"""Derive light/dark beveled artwork and compile one native macOS icon with automatic appearances."""

import argparse
import math
from pathlib import Path
import shutil
import subprocess
import tempfile
import xml.etree.ElementTree as ET

SVG = "http://www.w3.org/2000/svg"
ET.register_namespace("", SVG)
ET.register_namespace("xlink", "http://www.w3.org/1999/xlink")
BEVEL_WIDTH = 1.4  # Projected source units; about 3.6 pixels in the 1024 export.
LOGO_WIDTH = 0.90  # Visual-review choice, not an Apple sizing requirement.
MINIMUM_MACOS_VERSION = "15.0"  # Match the native bundle's oldest supported runtime.


def bevel_faces(points, width=BEVEL_WIDTH, dark=False):
    """Inset the concave face and shade each opaque chamfer toward upper-left light."""
    area = sum(a[0] * b[1] - b[0] * a[1] for a, b in zip(points, points[1:] + points[:1]))
    if area <= 0:
        raise ValueError("expected clockwise face in SVG's downward-y coordinate grid")
    normals = []
    for a, b in zip(points, points[1:] + points[:1]):
        dx, dy = b[0] - a[0], b[1] - a[1]
        length = math.hypot(dx, dy)
        normals.append((-dy / length, dx / length))
    inset = []
    for i, (x, y) in enumerate(points):
        a, b = normals[i - 1], normals[i]
        # Intersection of the two adjacent inward-offset edge lines. Concave
        # corners stay sharp without drawing a stroke outside the original X.
        determinant = a[0] * b[1] - a[1] * b[0]
        if abs(determinant) < 1e-8:
            raise ValueError("collinear face edges")
        inset.append((x + width * (b[1] - a[1]) / determinant,
                      y + width * (a[0] - b[0]) / determinant))
    faces = []
    light = (-0.6, -0.8)
    for i, inward in enumerate(normals):
        diffuse = -inward[0] * light[0] - inward[1] * light[1]
        base, lit, shaded = ((190, 199, 212), (250, 252, 255), (112, 125, 145)) if dark else ((42, 50, 63), (104, 116, 132), (16, 21, 29))
        low, high = base, lit if diffuse >= 0 else shaded
        color = tuple(round(a + (b - a) * abs(diffuse)) for a, b in zip(low, high))
        j = (i + 1) % len(points)
        faces.append(([points[i], points[j], inset[j], inset[i]], color))
    return faces


def foreground(assets, dark=False):
    logo = ET.parse(assets / "logo.svg").getroot()
    face = logo.find(f".//{{{SVG}}}polygon[@id='x-face']")
    points = [tuple(map(float, p.split(','))) for p in face.attrib["points"].split()]
    top = logo.find(f"{{{SVG}}}g[@id='black-layer']")
    if dark:
        # Only the top slab needs a dark variant. Keep the colored layers and
        # geometry unchanged so both appearances remain the same icon.
        gradient = logo.find(f".//{{{SVG}}}linearGradient[@id='top-light']")
        for stop, color in zip(gradient, ["#f4f6fa", "#dbe1ea", "#adb9cb"]):
            stop.set("stop-color", color)
        sides = {"#252d38": "#a1aec1", "#202630": "#8c9bb1", "#121720": "#697b95"}
        for polygon in top.findall(f"{{{SVG}}}polygon"):
            polygon.set("fill", sides[polygon.attrib["fill"]])
        logo.find(f"{{{SVG}}}desc").text = "Orthographic X slabs with a softly lit silver face above unchanged cobalt blue, yellow-green, and vermilion layers."
    bevel = ET.SubElement(top, f"{{{SVG}}}g", {"id": "directional-bevel"})
    for polygon, color in bevel_faces(points, dark=dark):
        ET.SubElement(bevel, f"{{{SVG}}}polygon", {
            "points": " ".join(f"{x:.4f},{y:.4f}" for x, y in polygon),
            "fill": "#{:02x}{:02x}{:02x}".format(*color),
        })
    width = 1024 * LOGO_WIDTH
    inset = (1024 - width) / 2
    logo.attrib.update(x=str(inset), y=str(inset), width=str(width), height=str(width))
    canvas = ET.Element(f"{{{SVG}}}svg", width="1024", height="1024", viewBox="0 0 1024 1024")
    canvas.append(logo)
    ET.indent(canvas)
    name = "foreground-dark.svg" if dark else "foreground.svg"
    target = assets / "Foveonic.icon/Assets" / name
    target.parent.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(canvas).write(target, encoding="unicode", xml_declaration=False)


def compile_icon(assets, output):
    """Write the native asset catalog, fallback ICNS, and compiler-owned icon metadata."""
    foreground(assets)
    foreground(assets, dark=True)
    output.mkdir(parents=True, exist_ok=True)
    # Composer previews omit Dock margins. Use the production compiler so
    # macOS supplies the correct padding, sizes, and automatic appearances.
    subprocess.run([
        "xcrun", "actool", "--compile", str(output), "--platform", "macosx",
        "--minimum-deployment-target", MINIMUM_MACOS_VERSION, "--app-icon", "Foveonic",
        "--standalone-icon-behavior", "all",
        "--output-partial-info-plist", str(output / "partial.plist"),
        str(assets / "Foveonic.icon"),
    ], check=True)


def export_png(compiled, output, size=1024):
    """Extract the default static rendition for non-macOS window icons and review."""
    with tempfile.TemporaryDirectory(prefix="foveonic-iconset-") as work:
        iconset = Path(work) / "Foveonic.iconset"
        subprocess.run(["iconutil", "-c", "iconset", str(compiled / "Foveonic.icns"), "-o", str(iconset)], check=True)
        names = {
            16: "icon_16x16.png", 32: "icon_32x32.png", 64: "icon_32x32@2x.png",
            128: "icon_128x128.png", 256: "icon_256x256.png",
            512: "icon_512x512.png", 1024: "icon_512x512@2x.png",
        }
        shutil.copyfile(iconset / names[size], output)


def generate():
    assets = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--size", type=int, choices=[16, 32, 64, 128, 256, 512, 1024], default=1024)
    parser.add_argument("--output", type=Path, default=assets / "icon.png")
    parser.add_argument("--compile-dir", type=Path, help="retain compiled resources for an app bundle")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="foveonic-icon-") as work:
        compiled = (args.compile_dir or Path(work)).resolve()
        compile_icon(assets, compiled)
        export_png(compiled, args.output, args.size)


if __name__ == "__main__":
    generate()
