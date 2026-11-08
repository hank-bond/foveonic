"""Render photographic help and standalone comparison grids through the shared CLI."""

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


def basename(name):
    if (not isinstance(name, str) or not name or name in ('.', '..')
            or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._' for c in name)):
        raise ValueError(f'Expected a plain filename, got {name!r}')
    return name


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


def render_images(data, source, renderer, work, tile_width=600):
    from PIL import Image
    # Concurrent builds must not change the renderer between variants.
    frozen = work / 'renderer'
    shutil.copy2(renderer, frozen)
    dimensions = None
    for index, variant in enumerate(data['variants'], 1):
        tiff = work / f'{index}.tiff'
        args = [*data['base_args'], *variant['args']]
        print(f"Rendering {index}/{len(data['variants'])}: {variant['label']}", flush=True)
        subprocess.run([str(frozen), 'render', str(source), str(tiff), *args], check=True)
        image = display_image(tiff)
        if dimensions is not None and image.size != dimensions:
            raise ValueError('Variants must render the same photograph dimensions')
        dimensions = image.size
        crop = data.get('crop', [0, 0, *image.size])
        if crop[2] > image.width or crop[3] > image.height:
            raise ValueError('Crop extends outside the oriented image')
        image = image.crop(crop)
        if 'image_size' not in data:
            data['image_size'] = [tile_width, max(1, round(tile_width * image.height / image.width))]
        image = image.resize(data['image_size'], Image.Resampling.LANCZOS)
        tiff.unlink()
        yield variant, image


def render(identifier, source_dir, renderer, write):
    directory = CATALOG / basename(identifier)
    data = load_manifest(directory / 'comparison.json')
    if data['id'] != identifier:
        raise ValueError('Comparison directory must match its ID')
    source = source_dir / data['source']
    if not source.is_file():
        raise ValueError(f'Missing {source}; run cargo xtask examples sync first')
    output = ROOT / '.local/help-renders' / identifier
    output.mkdir(parents=True, exist_ok=True)
    images = []
    with tempfile.TemporaryDirectory(prefix='render-', dir=output) as temporary:
        work = Path(temporary)
        for variant, image in render_images(data, source, renderer, work):
            image.save(work / variant['image'])
            images.append(image)
        for variant in data['variants']:
            shutil.copy2(work / variant['image'], output / variant['image'])
            if write:
                shutil.copy2(work / variant['image'], directory / variant['image'])
    comparison_sheet(data, images, output / 'review.png', len(images), False)
    print(f'Review: {output / "review.png"}')
    print('Updated checked-in assets.' if write else 'Candidates only; use --write after review to update assets.')


def grid_order(count, columns, clockwise):
    if count < 2 or columns < 1 or columns > count:
        raise ValueError('Use at least two variants and 1..variant-count columns')
    if clockwise:
        if count != 2 * columns:
            raise ValueError('Clockwise layout requires two complete rows')
        return [*range(columns), *reversed(range(columns, count))]
    return list(range(count))


def comparison_sheet(data, images, output, columns, clockwise):
    from PIL import Image, ImageDraw, ImageFont
    order = grid_order(len(images), columns, clockwise)
    width, height = data['image_size']
    margin, gap = 24, 16
    canvas_width = columns * width + (columns - 1) * gap + 2 * margin
    title_font = ImageFont.load_default(size=30)
    label_font = ImageFont.load_default(size=22)
    note_font = ImageFont.load_default(size=17)
    probe = ImageDraw.Draw(images[0])

    def lines(text, font, available):
        wrapped = ['']
        for word in text.split():
            candidate = (wrapped[-1] + ' ' + word).strip()
            if wrapped[-1] and probe.textlength(candidate, font=font) > available:
                wrapped.append(word)
            else:
                wrapped[-1] = candidate
        return '\n'.join(wrapped)

    title = lines(data['title'], title_font, canvas_width - 2 * margin)
    note = lines(data.get('note', ''), note_font, canvas_width - 2 * margin)
    labels = [lines(v['label'], label_font, width) for v in data['variants']]
    captions = [lines(v.get('caption', ''), note_font, width) for v in data['variants']]
    title_height = probe.multiline_textbbox((0, 0), title, font=title_font)[3] + 16
    label_height = max(probe.multiline_textbbox((0, 0), label, font=label_font)[3] for label in labels) + 12
    caption_height = max((probe.multiline_textbbox((0, 0), text, font=note_font)[3] + 12
                          for text in captions if text), default=0)
    tile_height = label_height + height + caption_height
    rows = (len(images) + columns - 1) // columns
    note_height = probe.multiline_textbbox((0, 0), note, font=note_font)[3] + 8 if note else 0
    canvas_height = 2 * margin + title_height + rows * tile_height + (rows - 1) * gap + note_height
    sheet = Image.new('RGB', (canvas_width, canvas_height), '#17191b')
    draw = ImageDraw.Draw(sheet)
    draw.multiline_text((margin, margin), title, font=title_font, fill='#eef0f2')
    for position, index in enumerate(order):
        x = margin + position % columns * (width + gap)
        y = margin + title_height + position // columns * (tile_height + gap)
        draw.multiline_text((x, y), labels[index], font=label_font, fill='#eef0f2')
        sheet.paste(images[index], (x, y + label_height))
        if captions[index]:
            draw.multiline_text((x, y + label_height + height + 8), captions[index], font=note_font, fill='#b8bdc2')
    if note:
        draw.multiline_text((margin, canvas_height - margin - note_height + 8), note, font=note_font, fill='#b8bdc2')
    from PIL import ImageCms
    sheet.save(output, icc_profile=ImageCms.ImageCmsProfile(ImageCms.createProfile('sRGB')).tobytes())


def compare(source, settings, renderer, output, columns, clockwise, tile_width):
    data = json.loads(settings.read_text())
    if not isinstance(data.get('title'), str) or not data['title'].strip():
        raise ValueError('Comparison needs a title')
    variants = data.get('variants', [])
    if not isinstance(variants, list) or not isinstance(data.get('note', ''), str):
        raise ValueError('Use a variants list and a text note')
    if columns is None:
        columns = min(3, len(variants))
    data.pop('image_size', None)
    grid_order(len(variants), columns, clockwise)
    if tile_width < 1:
        raise ValueError('Tile width must be positive')
    for variant in variants:
        if not isinstance(variant.get('label'), str) or not variant['label'].strip():
            raise ValueError('Each variant needs a label')
    data.setdefault('base_args', [])
    for args in [data['base_args'], *(v.get('args') for v in variants)]:
        if not isinstance(args, list) or not all(isinstance(v, str) for v in args):
            raise ValueError('Render arguments must be lists of strings')
    if 'crop' in data:
        crop = data['crop']
        if (len(crop) != 4 or any(type(v) is not int or v < 0 for v in crop)
                or crop[2] <= crop[0] or crop[3] <= crop[1]):
            raise ValueError('Crop needs positive exclusive oriented-image bounds')
    if output.suffix.lower() != '.png':
        raise ValueError('Comparison output must be a PNG')
    if not source.is_file() or not renderer.is_file():
        raise ValueError('Source and built renderer must exist')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='compare-', dir=output.parent) as temporary:
        images = [image for _, image in render_images(data, source, renderer, Path(temporary), tile_width)]
        comparison_sheet(data, images, output, columns, clockwise)
    print(f'Comparison: {output}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source-dir', type=Path, default=SOURCES)
    commands = parser.add_subparsers(dest='command', required=True)
    rendering = commands.add_parser('render', help='Explicitly render one comparison')
    rendering.add_argument('id')
    rendering.add_argument('--renderer', type=Path, default=ROOT / 'target/release/foveonic')
    rendering.add_argument('--write', action='store_true', help='Also update the checked-in PNGs')
    comparing = commands.add_parser('compare', help='Render a standalone comparison from a source and settings list')
    comparing.add_argument('source', type=Path)
    comparing.add_argument('settings', type=Path)
    comparing.add_argument('--output', type=Path, required=True)
    comparing.add_argument('--renderer', type=Path, default=ROOT / 'target/release/foveonic')
    comparing.add_argument('--columns', type=int, help='Columns in the grid, up to three by default')
    comparing.add_argument('--clockwise', action='store_true', help='Put the second row in reverse order')
    comparing.add_argument('--tile-width', type=int, default=600)
    args = parser.parse_args()
    if args.command == 'compare':
        compare(args.source.resolve(), args.settings.resolve(), args.renderer.resolve(),
                args.output.resolve(), args.columns, args.clockwise, args.tile_width)
    else:
        render(args.id, args.source_dir.resolve(), args.renderer.resolve(), args.write)


if __name__ == '__main__':
    main()
