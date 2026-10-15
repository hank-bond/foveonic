"""Download official sample files and explicitly render manifest-driven photographic help."""

import argparse
import io
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CATALOG = ROOT / 'crates/foveonic-gui/assets/help'
SOURCES = ROOT / '.local/example-photographs'
REPO = 'hank-bond/foveonic'
RELEASE = 'example-photographs'


def basename(name):
    if (not isinstance(name, str) or not name or name in ('.', '..')
            or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._' for c in name)):
        raise ValueError(f'Expected a plain filename, got {name!r}')
    return name


def sync(destination):
    destination.mkdir(parents=True, exist_ok=True)
    subprocess.run([
        'gh', 'release', 'download', RELEASE, '--repo', REPO,
        '--dir', str(destination), '--pattern', '*.x3f', '--skip-existing',
    ], check=True)
    print(f'Downloaded missing example photographs to {destination}')


def load_manifest(path):
    data = json.loads(path.read_text())
    basename(data['id'])
    basename(data['source'])
    if not data['source'].endswith('.x3f'):
        raise ValueError('Source must name an official X3F')
    for field in ('title', 'description', 'note'):
        if not isinstance(data[field], str) or not data[field].strip():
            raise ValueError(f'Missing {field}')
    crop, size = data['crop'], data['image_size']
    if (len(crop) != 4 or any(type(v) is not int or v < 0 for v in crop)
            or crop[2] <= crop[0] or crop[3] <= crop[1]):
        raise ValueError('Crop must have positive, exclusive oriented-image bounds')
    if len(size) != 2 or any(type(v) is not int or v <= 0 for v in size):
        raise ValueError('Image size must contain two positive integers')
    if len(data['variants']) < 2:
        raise ValueError('A comparison needs at least two variants')
    ids, images = set(), set()
    for variant in data['variants']:
        name, image = basename(variant['id']), basename(variant['image'])
        if name in ids or image in images or not image.endswith('.png'):
            raise ValueError('Variant IDs and PNG filenames must be distinct')
        ids.add(name)
        images.add(image)
        if not variant['label'].strip() or not variant['caption'].strip():
            raise ValueError('Each variant needs a label and caption')
    for args in [data['base_args'], *(v['args'] for v in data['variants'])]:
        if not isinstance(args, list) or not all(isinstance(v, str) for v in args):
            raise ValueError('Render arguments must be lists of strings')
    return data


def display_image(path):
    # Pillow is only required by this explicit generation step, never by builds.
    from PIL import Image, ImageCms
    with Image.open(path) as image:
        if image.getexif().get(274, 1) != 1:
            raise ValueError('Renderer must return an already-oriented TIFF')
        profile = image.info.get('icc_profile')
        if not profile:
            raise ValueError('Rendered TIFF is missing its color profile')
        return ImageCms.profileToProfile(
            image, ImageCms.ImageCmsProfile(io.BytesIO(profile)),
            ImageCms.createProfile('sRGB'), outputMode='RGB')


def review_sheet(data, images, path):
    from PIL import Image, ImageDraw, ImageFont
    width, height = data['image_size']
    # Pillow's bundled font makes review sheets portable across operating systems.
    font = ImageFont.load_default(size=18)
    small = ImageFont.load_default(size=14)
    sheet = Image.new('RGB', (len(images) * (width + 16) + 16, height + 190), '#202020')
    draw = ImageDraw.Draw(sheet)
    draw.text((16, 12), data['title'], font=font, fill='white')
    draw.text((16, 40), data['note'].replace('−', '-'), font=small, fill='#dddddd')
    for index, (variant, image) in enumerate(zip(data['variants'], images)):
        x = 16 + index * (width + 16)
        draw.text((x, 70), variant['label'], font=font, fill='white')
        sheet.paste(image, (x, 100))
        lines = ['']
        for word in variant['caption'].split():
            candidate = (lines[-1] + ' ' + word).strip()
            if draw.textlength(candidate, font=small) > width:
                lines.append(word)
            else:
                lines[-1] = candidate
        draw.multiline_text((x, height + 112), '\n'.join(lines), font=small, fill='#dddddd')
    sheet.save(path)


def render(identifier, source_dir, renderer, write):
    from PIL import Image
    directory = CATALOG / basename(identifier)
    manifest_path = directory / 'comparison.json'
    data = load_manifest(manifest_path)
    if data['id'] != identifier:
        raise ValueError('Comparison directory must match its ID')
    source = source_dir / data['source']
    if not source.is_file():
        raise ValueError(f'Missing {source}; run the sync command first')
    output = ROOT / '.local/help-renders' / identifier
    output.mkdir(parents=True, exist_ok=True)
    images = []
    with tempfile.TemporaryDirectory(prefix='render-', dir=output) as temporary:
        work = Path(temporary)
        # Freeze the executable so concurrent builds cannot mix renderer versions.
        frozen = work / 'renderer'
        shutil.copy2(renderer, frozen)
        for variant in data['variants']:
            tiff = work / (variant['id'] + '.tiff')
            args = [*data['base_args'], *variant['args']]
            subprocess.run([str(frozen), 'render', str(source), str(tiff), *args], check=True)
            image = display_image(tiff)
            if data['crop'][2] > image.width or data['crop'][3] > image.height:
                raise ValueError('Crop extends outside the oriented image')
            image = image.crop(data['crop']).resize(data['image_size'], Image.Resampling.LANCZOS)
            image.save(work / variant['image'])
            images.append(image)
        for name in (variant['image'] for variant in data['variants']):
            shutil.copy2(work / name, output / name)
            if write:
                shutil.copy2(work / name, directory / name)
    review_sheet(data, images, output / 'review.png')
    print(f'Review: {output / "review.png"}')
    print('Updated checked-in assets.' if write else 'Candidates only; use --write after review to update assets.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-dir', type=Path, default=SOURCES)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('sync', help='Download missing official X3Fs')
    rendering = commands.add_parser('render', help='Explicitly render one comparison')
    rendering.add_argument('id')
    rendering.add_argument('--renderer', type=Path, default=ROOT / 'target/release/foveonic')
    rendering.add_argument('--write', action='store_true', help='Also update the checked-in PNGs')
    args = parser.parse_args()
    if args.command == 'sync':
        sync(args.source_dir.resolve())
    else:
        render(args.id, args.source_dir.resolve(), args.renderer.resolve(), args.write)


if __name__ == '__main__':
    main()
