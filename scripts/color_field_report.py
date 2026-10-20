"""Build a private, color-managed, toggleable report from saved-edit GPU comparisons."""
import argparse
import collections
import copy
import hashlib
import html
import io
import json
import os
from pathlib import Path
import statistics
import sys

LABELS = {
    'stock-saved': 'Classic · saved threshold',
    'stock-auto': 'Classic · Auto threshold',
    'color-field': 'Color Field: Global',
    'active-area': 'Active-area grid · current bounds',
    'roi-baseline': 'Active-area grid · initial bounds',
    'anchors': 'Anchors only · no gradient solve',
    'uniform-support': 'Uniform support · no distance solve',
    'coarse-grid': '16px color grid',
    'fast-solve': 'Fast solve · looser residual bounds',
    'local-gradient': 'Color Field: Local',
    'fine-grid': '4px color grid · finer boundaries',
    'strict-donation': 'Stricter donation · agreement 0.5 / boundary 0.4 EV',
    'half-donation': 'Half color donation',
    'local-ratios': 'No donated color · local ratios',
}
PR_REGIONS = json.loads(Path(__file__).with_name('color_field_pr_regions.json').read_text())


def region_boxes(scene, dimensions):
    entry = PR_REGIONS['scenes'].get(scene)
    if entry is None:
        return []
    if list(dimensions) != entry['dimensions']:
        raise ValueError(f'PR registration requires native oriented dimensions: {scene}')
    width, height = dimensions
    regions = []
    for region in entry['regions']:
        left, top, right, bottom = region['box']
        if not (0 <= left < right <= width and 0 <= top < bottom <= height):
            raise ValueError(f'Invalid PR crop bounds: {scene} {region["id"]}')
        regions.append(dict(region, reference_url='https://github.com/user-attachments/assets/'+region['attachment']))
    return regions


def sha256(path):
    h = hashlib.sha256()
    with path.open('rb') as f:
        for block in iter(lambda: f.read(4 * 1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()


def summarize(records):
    grouped = collections.defaultdict(list)
    for r in records:
        if r['event'] == 'render':
            grouped[r['scene'], r['variant']].append(r)
    result = {}
    for (scene, variant), samples in grouped.items():
        warm = [r['completed_ms'] for r in samples if r['mode'] == 'wall' and not r['first']]
        first = [r['completed_ms'] for r in samples if r['mode'] == 'wall' and r['first']]
        gpu = collections.defaultdict(float)
        for r in samples:
            if r['mode'] == 'gpu':
                for stage in r['profile']['gpu']:
                    gpu[stage['stage']] += stage['milliseconds']
        result[scene, variant] = dict(
            first_ms=first[0] if first else None,
            warm_median_ms=statistics.median(warm) if warm else None,
            warm_min_ms=min(warm) if warm else None,
            warm_max_ms=max(warm) if warm else None,
            warm_samples=len(warm), gpu_stages=dict(gpu),
            settings=samples[0]['settings'], sidecar_used=samples[0]['sidecar_used'],
            activity=samples[-1]['activity'], dataset=samples[0].get('dataset'),
            source_settings=samples[0].get('source_settings'), integration_note=samples[0].get('integration_note'),
        )
    return result


def make_assets(tiff, assets, scene, variant, native=False):
    from PIL import Image, ImageCms
    assets.mkdir(exist_ok=True)
    srgb = ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB'))
    profile = srgb.tobytes()
    with Image.open(tiff) as image:
        icc = image.info.get('icc_profile')
        if not icc:
            raise ValueError(f'Missing input color profile: {tiff}')
        width, height = image.size
        full = assets / f'{variant}-full.webp'
        native_path = assets / f'{variant}-native.webp' if native else None
        crops = []
        for region in region_boxes(scene, (width, height)):
            box = region['box']
            extent = '-'.join(map(str, box))
            crops.append(dict(region, path=f'{variant}-pr-{region["id"]}-{extent}.webp'))
        modified = tiff.stat().st_mtime_ns
        def stale(path):
            return not path.exists() or path.stat().st_mtime_ns < modified
        if native_path and stale(native_path):
            image.convert('RGB').save(native_path, lossless=True, method=1, icc_profile=icc)
        if stale(full) or any(stale(assets / c['path']) for c in crops):
            converted = ImageCms.profileToProfile(image.convert('RGB'), ImageCms.ImageCmsProfile(io.BytesIO(icc)), srgb, outputMode='RGB')
            if stale(full):
                preview = converted.copy()
                preview.thumbnail((1800, 1800), Image.Resampling.LANCZOS)
                preview.save(full, lossless=True, method=1, icc_profile=profile)
            for crop in crops:
                path = assets / crop['path']
                if stale(path):
                    converted.crop(crop['box']).save(path, lossless=True, method=1, icc_profile=profile)
        return dict(dimensions=[width, height], full=full.name, crops=crops,
                    native_profile=native_path.name if native_path else None, preview_bits=8,
                    source_bits=list(getattr(image, 'tag_v2', {}).get(258, ())))


def stage_preview(source, scene, variant):
    """Convert only an explicitly tool-owned staging TIFF, never a retained artifact."""
    from PIL import Image
    if source.name != f'.staging-{variant}.tiff' or source.parent.name != scene:
        raise ValueError('only the matching scene/variant staging TIFF may be consumed')
    metadata = source.parent / f'{variant}-assets.json'
    if metadata.exists():
        raise FileExistsError(metadata)
    result = make_assets(source, source.parent / 'images', scene, variant, native=True)
    result['source_tiff_sha256'] = sha256(source)
    for name in [result['full'], result['native_profile'], *(c['path'] for c in result['crops'])]:
        with Image.open(source.parent / 'images' / name) as image:
            image.load()
            if not image.info.get('icc_profile'):
                raise ValueError(f'Preview lost its color profile: {name}')
    with metadata.open('x') as file:
        json.dump(result, file, indent=2)
        file.write('\n')
    source.unlink()
    return result


def build(directory, corpus, supplements=(), baseline=None, roi_supplements=(), findings=None):
    records, locations, sessions = [], {}, []
    sources = [(directory, False), *((source, False) for source in supplements), *((source, True) for source in roi_supplements)]
    if baseline is not None:
        sources.append((baseline, True))
    for source, initial_prototype in sources:
        loaded = [json.loads(line) for line in (source / 'measurements.jsonl').read_text().splitlines()]
        binary = source / 'renderer'
        sessions.append(dict(directory=source.name, context=loaded[0], renderer_sha256=sha256(binary) if binary.exists() else None))
        for record in loaded:
            record['dataset'] = source.name
            original_variant = record.get('variant')
            if initial_prototype and original_variant not in (None, 'stock-saved', 'stock-auto'):
                record['variant'] = 'roi-baseline' if original_variant == 'color-field' else f'roi-{original_variant}'
                record['integration_note'] = 'Measured with active-area grid alignment and initial stopping bounds (brightness 1e-6, color 2e-6; Fast solve uses its separate relaxed bounds). Current brightness is tighter (5e-7); color remains 2e-6 after the 1e-6 trial hit float32 residual floors after full-raster CPU validation. Enable Active-area only for matching geometry/tuning; use the frozen recorded binary for exact numerical/performance reproduction.'
                if 'settings' in record:
                    record['source_settings'] = record['settings']
                    record['settings'] = copy.deepcopy(record['settings'])
                    record['settings']['color_field']['area'] = 'active'
            if record['event'] == 'render':
                key = record['scene'], record['variant']
                location = source, original_variant
                if key in locations and locations[key] != location:
                    raise ValueError(f'Duplicate recipe across datasets: {key}')
                locations[key] = location
        records.extend(loaded)
    summary = summarize(records)
    manifest_path = corpus.parent / 'manifest.json'
    manifest = json.loads(manifest_path.read_text()) if manifest_path.exists() else {'images': []}
    originals = {Path(i['filename']).stem: i for i in manifest['images']}
    scenes = {}
    for (scene, variant), stats in sorted(summary.items()):
        source, original_variant = locations[scene, variant]
        tiff = source / scene / f'{original_variant}.tiff'
        assets = source / scene / 'images'
        if tiff.exists():
            preview = make_assets(tiff, assets, scene, variant)
            native_tiff = os.path.relpath(tiff, directory)
        else:
            metadata = source / scene / f'{original_variant}-assets.json'
            if not metadata.exists():
                continue
            preview = json.loads(metadata.read_text())
            # Old staged records keep their original crops as evidence. Reframe
            # from their lossless native-profile pixels without a GPU rerender.
            native = assets / preview['native_profile']
            reframed = make_assets(native, assets, scene, variant)
            preview = dict(preview, crops=reframed['crops'])
            native_tiff = None
        width, height = preview['dimensions']
        full = assets / preview['full']
        crops = [dict(c, path=os.path.relpath(assets / c['path'], directory)) for c in preview['crops']]
        native_preview = os.path.relpath(assets / preview['native_profile'], directory) if preview.get('native_profile') else None
        source = originals.get(scene, {})
        entry = scenes.setdefault(scene, dict(name=scene, description=source.get('scene', ''), source_sha256=source.get('sha256'), dimensions=[width, height], regions=region_boxes(scene, (width, height)), variants={}))
        sidecar = corpus / f'{scene}.X3F.foveonic.json'
        if entry['dimensions'] != [width, height]:
            raise ValueError(f'Variant dimensions differ: {scene} {variant}')
        entry['sidecar_sha256'] = sha256(sidecar) if sidecar.exists() else None
        entry['variants'][variant] = dict(stats, full=os.path.relpath(full, directory), crops=crops, tiff=native_tiff, native_preview=native_preview, source_tiff_sha256=preview.get('source_tiff_sha256'), preview_bits=8)
    errors = [r for r in records if r['event'] == 'error']
    present = {variant for scene in scenes.values() for variant in scene['variants']}
    labels = {}
    for key, label in LABELS.items():
        if key in present:
            labels[key] = label
        if f'roi-{key}' in present:
            labels[f'roi-{key}'] = f'{label} · active area'
    for key in sorted(present - labels.keys()):
        labels[key] = key
    if not scenes:
        raise ValueError('No successful comparison assets')
    data = dict(session=records[0], sessions=sessions, scenes=scenes, labels=labels, errors=errors, crop_registration=PR_REGIONS['registration'], findings=json.loads(findings.read_text()) if findings else {})
    (directory / 'report-data.json').write_text(json.dumps(data, indent=2) + '\n')
    payload = json.dumps(data).replace('</', '<\\/')
    (directory / 'index.html').write_text(PAGE.replace('__DATA__', payload))
    print(directory / 'index.html')


PAGE = '''<!doctype html><html lang="en"><meta charset="utf-8"><title>Color-field highlight review</title>
<style>
:root{color-scheme:dark;font:15px system-ui}body{margin:0;background:#17191c;color:#e7e8eb}header,main{padding:18px 28px}header{background:#242830}h1{font-size:23px;margin:0 0 10px}p{max-width:1100px;line-height:1.5}.muted{color:#adb5c1}.bar{display:flex;gap:10px;flex-wrap:wrap;align-items:center;margin:12px 0}select,button{font:inherit;padding:7px 10px;background:#303744;color:inherit;border:1px solid #647086;border-radius:5px}button.active{background:#205c8c}.viewer{display:flex;gap:14px;align-items:flex-start}.pane{flex:1;min-width:0;text-align:center}.pane img{max-width:100%;max-height:80vh;object-fit:contain;background:#111;cursor:pointer}#controls{position:sticky;top:0;background:#17191c;z-index:5;padding:5px 0;border-bottom:1px solid #3c4655}.viewer.native .pane{overflow:auto}.viewer.native img{max-width:none;max-height:none}.pane h3{font-size:15px;margin:5px}.metric{font-size:13px;color:#bac9da;min-height:22px}table{border-collapse:collapse;width:100%;font-variant-numeric:tabular-nums}td,th{padding:8px;text-align:left;border-bottom:1px solid #3c4655}tr.selected{background:#233b50}pre{white-space:pre-wrap;overflow-wrap:anywhere;font-size:12px}details{margin:15px 0}a{color:#8fcaff}.warning{color:#ffd388}.scroll{overflow:auto}#notes{border-left:3px solid #638eb8;padding-left:14px}.error{color:#ffb1aa}
</style>
<header><h1>Color-field highlight reconstruction · private review</h1>
<p>Saved photographic edits, unchanged except for the selected highlight method or explicitly labeled experiment. Compare <b>Classic with your saved threshold</b> against the faithful GPU Color field and deliberate simplifications. This is a review candidate, not an accepted default.</p>
<p class="muted">Space or click the image toggles A/B. Keys 1–9 select the first nine approaches. Arrow keys change scene. Named PR regions use the screenshot's framing, not arbitrary 640px squares. Registered bounds are estimated and visually checked, not the unpublished original coordinates. Crop assets retain native pixels. Use Fit / 100% to inspect them without display scaling. Scenes without a PR detail shot offer full frame only. Full views are reduced to 1800 pixels. Browser assets are color-managed sRGB, lossless WebP after 8-bit preview conversion. Numerical validation uses floating data, not these previews. sRGB gamut limits can hide saturated flower differences. Use the linked native-profile TIFFs or native-profile 8-bit previews for gamut-critical review. Preview-only experiments retain all native pixels at 8-bit precision, not RGB16 TIFFs; the complete saved recipe and frozen renderer can regenerate those.</p>
<div id="notes"><p>Priorities: <b>0095</b> magenta-petal blotches, <b>0286</b> color donation into white sweater stripes, and <b>0313</b> strongly overexposed skin and swaddle. Use 0170 to watch the known striped-skirt failure. Lower donation/agreement can suppress contamination but also remove genuine color.</p>
<p id="status" class="error" hidden></p><p id="methodology" class="warning"></p><p class="warning">The pinned CPU reference supplies the recovery equations. Calibration and downstream rendering remain Foveonic's. These are not promised pixel matches to the PR screenshots or Sigma Photo Pro. Algorithm variants are not automatically equivalent to global reconstruction.</p></div>
</header><main><section id="controls">
<div class="bar"><label>Scene <select id="scene"></select></label><label>View <select id="view"></select></label><button id="zoom">Fit / 100%</button><button id="layout">Side by side</button><a id="pr-link" target="_blank" rel="noopener" hidden>PR screenshot</a></div>
<h2 id="title"></h2><div class="bar"><label>A <select id="a"></select></label><label>B <select id="b"></select></label><button id="toggle">Toggle A/B</button><span id="visible"></span></div>
</section><div class="viewer"><div class="pane" id="paneA"><h3 id="labelA"></h3><div class="metric" id="metricA"></div><img id="imgA" alt="Approach A"><div id="linkA"></div></div><div class="pane" id="paneB"><h3 id="labelB"></h3><div class="metric" id="metricB"></div><img id="imgB" alt="Approach B"><div id="linkB"></div></div></div>
<section id="findings" hidden><h2>Review findings and validation</h2><div id="findings-body"></div></section>
<h2>Completed rendering</h2><p class="muted">Fresh full processing from decoded source with an explicit GPU completion wait. Optional Color field shaders compile only when first selected, so its first sample can include compilation. Warm medians exclude the first iteration and synchronized GPU diagnostics. Export, decoding, preview creation and engine initialization are outside these render intervals. The first recipe sample is not a new GPU process. Allocation/transfer totals are not peak resident memory.</p>
<div class="scroll"><table><thead><tr><th>Approach</th><th>First ms</th><th>Warm median ms</th><th>Warm range</th><th>n</th></tr></thead><tbody id="timings"></tbody></table></div>
<details><summary>GPU stage timings · separate synchronized diagnostic</summary><div id="gpu"></div></details>
<details><summary>Exact settings, source/sidecar hashes, crop coordinates and engine context</summary><pre id="recipe"></pre></details>
<details><summary>Errors and incomplete cases</summary><pre id="errors"></pre></details>
<p class="muted">The supplemental 0009 and 0399 frames had no saved sidecars. Their explicitly chosen Sunlight/Standard, −1.5 EV recipes are labeled separately. No source files or owner sidecars were modified.</p>
</main><script>
const data=__DATA__;const $=s=>document.getElementById(s);let side=false,showB=true;
if(Object.keys(data.findings).length){$('findings').hidden=false;for(const [heading,paragraphs]of Object.entries(data.findings)){let h=document.createElement('h3');h.textContent=heading;$('findings-body').append(h);for(const text of paragraphs){let p=document.createElement('p');p.textContent=text;$('findings-body').append(p);}}}
const priority=['_P2M0095','_P2M0286','_P2M0313','_P2M0170'];const names=Object.keys(data.scenes).sort((a,b)=>{let x=priority.indexOf(a),y=priority.indexOf(b);return (x<0?99:x)-(y<0?99:y)||a.localeCompare(b)});
for(const n of names)$('scene').add(new Option(n+' · '+data.scenes[n].description,n));
const num=v=>v==null?'—':v.toFixed(1);const label=v=>data.labels[v]||v;
function variant(s,key){const error=data.errors.find(e=>e.scene===s.name&&e.variant===key);if(s.variants[key])return {...s.variants[key],error:error?.error};if(!error)return null;return {full:null,crops:[],tiff:null,native_preview:null,settings:error.settings,dataset:error.dataset,error:error.error,first_ms:null,warm_median_ms:null,warm_min_ms:null,warm_max_ms:null,warm_samples:0,gpu_stages:{},activity:null};}
if(data.errors.length){$('status').hidden=false;$('status').textContent='INCOMPLETE · '+data.errors.length+' failed render attempts in this report’s frozen measurement round. A failed approach has no substitute picture. See findings for any later validation.';}
$('methodology').textContent=Object.keys(data.labels).some(k=>k.startsWith('roi-'))?'Earlier ROI trials are explicitly labeled. Compare like work domains and solver revisions; recorded binaries and original settings remain disclosed.':'All current approaches use one frozen renderer and the full sensor raster except the explicitly named Active-area grid trial. Compare Global/Active-area grid to isolate work domain; other method and tuning trials share the full-raster baseline. No approximation was promoted to the application default.';
function choices(){let s=data.scenes[$('scene').value];let oldView=$('view').value;$('view').replaceChildren(new Option('Full frame','full'));for(const [i,r]of s.regions.entries())$('view').add(new Option('PR · '+r.label,'crop'+i));$('view').value=[...$('view').options].some(o=>o.value===oldView)?oldView:(s.regions.length?'crop0':'full');for(const id of ['a','b']){let old=$(id).value;$(id).replaceChildren();for(const v of Object.keys(data.labels)){let item=variant(s,v);if(item)$(id).add(new Option(label(v)+(item.error?' · FAILED':''),v));}$(id).value=variant(s,old)?old:(id==='a'&&variant(s,'stock-saved')?'stock-saved':variant(s,'color-field')?'color-field':Object.keys(s.variants)[0]);}render();}
function render(){let s=data.scenes[$('scene').value];if(!s)return;let region=$('view').value==='full'?null:s.regions[Number($('view').value.slice(4))];$('pr-link').hidden=!region;if(region){$('pr-link').href=region.reference_url;$('pr-link').textContent='PR screenshot · '+region.label;}$('title').textContent=s.name+' · '+s.description+(s.sidecar_sha256?'':' · no saved sidecar');
for(const id of ['A','B']){let key=$(id.toLowerCase()).value,v=variant(s,key);$('label'+id).textContent=label(key);$('metric'+id).textContent='Warm '+num(v.warm_median_ms)+' ms · first '+num(v.first_ms)+' ms';let path=v.full;if(path&&$('view').value!=='full'){let c=v.crops[Number($('view').value.slice(4))];path=c.path;$('metric'+id).textContent+=' · ['+c.box.join(', ')+')';}$('img'+id).hidden=!path;if(path)$('img'+id).src=path;else $('img'+id).removeAttribute('src');if(v.error)$('metric'+id).textContent+=(path?' · PARTIAL: ':' · FAILED: ')+v.error;$('metric'+id).classList.toggle('error',!!v.error);$('link'+id).replaceChildren();if(v.tiff||v.native_preview){let a=document.createElement('a');a.href=v.tiff||v.native_preview;a.target='_blank';a.rel='noopener';a.textContent=v.tiff?'Native RGB16 TIFF':'Native-profile preview · 8-bit';$('link'+id).append(a);}}
$('paneA').hidden=!side&&showB;$('paneB').hidden=!side&&!showB;$('visible').textContent=side?'Both approaches':showB?'Showing B':'Showing A';$('layout').classList.toggle('active',side);
$('timings').replaceChildren();for(const k of Object.keys(data.labels)){const v=variant(s,k);if(!v)continue;let tr=document.createElement('tr');if(k===$('b').value)tr.className='selected';for(const t of [label(k)+(v.error?' · FAILED':''),num(v.first_ms),num(v.warm_median_ms),num(v.warm_min_ms)+'–'+num(v.warm_max_ms),v.warm_samples]){let td=document.createElement('td');td.textContent=t;tr.append(td);}tr.onclick=()=>{$('b').value=k;showB=true;render()};$('timings').append(tr);}
let v=variant(s,$('b').value);$('gpu').replaceChildren();let t=document.createElement('table');for(const [k,ms]of Object.entries(v.gpu_stages).sort((a,b)=>b[1]-a[1])){let r=t.insertRow();r.insertCell().textContent=k;r.insertCell().textContent=num(ms)+' ms';}$('gpu').append(t);
$('recipe').textContent=JSON.stringify({sessions:data.sessions,datasets:{A:variant(s,$('a').value).dataset,B:v.dataset},display:side?'A/B':showB?'B':'A',settings_A:variant(s,$('a').value).settings,source_settings_B:v.source_settings,integration_note:v.integration_note,transient_tiff_sha256:v.source_tiff_sha256,preview_bits:v.preview_bits,source_sha256:s.source_sha256,sidecar_sha256:s.sidecar_sha256,dimensions:s.dimensions,crop_registration:data.crop_registration,selected_approach:$('b').value,settings:v.settings,activity:v.activity,crops:v.crops},null,2);$('errors').textContent=data.errors.length?JSON.stringify(data.errors,null,2):'No render errors recorded.';}
$('zoom').onclick=()=>{let active=document.querySelector('.viewer').classList.toggle('native');$('zoom').classList.toggle('active',active);$('zoom').setAttribute('aria-pressed',String(active));};
for(const id of ['view','a','b'])$(id).onchange=render;$('scene').onchange=choices;$('layout').onclick=()=>{side=!side;render()};const toggle=()=>{side=false;showB=!showB;render()};$('toggle').onclick=toggle;$('imgA').onclick=toggle;$('imgB').onclick=toggle;
document.onkeydown=e=>{if(/INPUT|SELECT|TEXTAREA/.test(e.target.tagName))return;if(e.code==='Space'){e.preventDefault();toggle()}if(e.key==='ArrowRight'||e.key==='ArrowLeft'){let i=names.indexOf($('scene').value);$('scene').value=names[(i+(e.key==='ArrowRight'?1:-1)+names.length)%names.length];choices()}let n=Number(e.key);if(n>=1&&n<=9){let k=Object.keys(data.labels)[n-1];if(variant(data.scenes[$('scene').value],k)){$('b').value=k;showB=true;render()}}};choices();
</script></html>'''

if __name__ == '__main__' and len(sys.argv)>1 and sys.argv[1]=='--stage':
    parser = argparse.ArgumentParser(description='Consume only a tool-owned staging TIFF after verifying color-managed report previews.')
    parser.add_argument('--stage',type=Path,required=True)
    parser.add_argument('--scene',required=True)
    parser.add_argument('--variant',required=True)
    args=parser.parse_args()
    print(json.dumps(stage_preview(args.stage,args.scene,args.variant)))
elif __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--corpus', type=Path, required=True)
    parser.add_argument('--supplement', type=Path, action='append', default=[])
    parser.add_argument('--baseline', type=Path, help='Initial active-area prototype dataset. Import stock controls and explicitly label its ROI field experiments.')
    parser.add_argument('--roi-supplement', type=Path, action='append', default=[], help='Additional pre-correction active-area experiment dataset.')
    parser.add_argument('--findings', type=Path, help='Private JSON object of review headings to paragraph lists, displayed as plain text.')
    args = parser.parse_args()
    build(args.directory, args.corpus, args.supplement, args.baseline, args.roi_supplement, args.findings)
