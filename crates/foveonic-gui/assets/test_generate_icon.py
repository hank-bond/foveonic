"""Check bevel lighting, preserved foreground geometry, and explicitly glass-free Composer settings."""

import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import xml.etree.ElementTree as ET

ASSETS = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("generate_icon", ASSETS / "generate-icon.py")
ICON = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ICON)


class IconTests(unittest.TestCase):
    def test_bevel_is_inward_and_follows_upper_left_light(self):
        square = [(0., 0.), (10., 0.), (10., 10.), (0., 10.)]
        faces = ICON.bevel_faces(square, 1.)
        self.assertEqual(faces[0][0], [(0., 0.), (10., 0.), (9., 1.), (1., 1.)])
        self.assertGreater(sum(faces[0][1]), sum(faces[2][1]))
        self.assertGreater(sum(faces[3][1]), sum(faces[1][1]))
        light_faces = ICON.bevel_faces(square, 1., dark=True)
        self.assertEqual([p for p, _ in faces], [p for p, _ in light_faces])
        self.assertGreater(sum(light_faces[0][1]), sum(light_faces[2][1]))
        self.assertGreater(sum(light_faces[3][1]), sum(light_faces[1][1]))
        with self.assertRaises(ValueError):
            ICON.bevel_faces(list(reversed(square)))

    def test_native_document_disables_glass_without_muting_source_colors(self):
        document = json.loads((ASSETS / "Foveonic.icon/icon.json").read_text())
        group = document["groups"][0]
        self.assertFalse(group["specular"])
        self.assertFalse(group["translucency"]["enabled"])
        self.assertEqual(group["shadow"]["opacity"], 0)
        self.assertFalse(group["layers"][0]["glass"])
        colors = document["fill-specializations"][0]["value"]["linear-gradient"]
        self.assertGreater(float(colors[0].split(":")[1].split(",")[0]), float(colors[1].split(":")[1].split(",")[0]))

    def test_one_document_specializes_background_and_foreground(self):
        document = json.loads((ASSETS / "Foveonic.icon/icon.json").read_text())
        # Plain fields take precedence over specializations in Composer. Do not
        # retain them as a second path: it silently ignores the dark artwork.
        self.assertNotIn("fill", document)
        light, dark = document["fill-specializations"]
        self.assertNotIn("appearance", light)
        self.assertEqual(dark["appearance"], "dark")
        self.assertEqual(light["value"]["orientation"], dark["value"]["orientation"])
        colors = dark["value"]["linear-gradient"]
        top, bottom = [list(map(float, c.split(":")[1].split(","))) for c in colors]
        self.assertTrue(all(0 <= b < t < 0.3 for t, b in zip(top[:3], bottom[:3])))
        self.assertEqual(top[3], 1)
        self.assertEqual(bottom[3], 1)
        layer = document["groups"][0]["layers"][0]
        self.assertNotIn("image-name", layer)
        self.assertEqual(layer["image-name-specializations"], [
            {"value": "foreground.svg"},
            {"appearance": "dark", "value": "foreground-dark.svg"},
        ])
        for variant in layer["image-name-specializations"]:
            self.assertTrue((ASSETS / "Foveonic.icon/Assets" / variant["value"]).is_file())

    def test_dark_foreground_preserves_colors_geometry_and_default_files(self):
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            shutil.copyfile(ASSETS / "logo.svg", assets / "logo.svg")
            ICON.foreground(assets)
            original = (assets / "Foveonic.icon/Assets/foreground.svg").read_bytes()
            ICON.foreground(assets, dark=True)
            self.assertEqual(original, (assets / "Foveonic.icon/Assets/foreground.svg").read_bytes())
            light = ET.fromstring(original).find(f"{{{ICON.SVG}}}svg")
            dark = ET.parse(assets / "Foveonic.icon/Assets/foreground-dark.svg").getroot().find(f"{{{ICON.SVG}}}svg")
            self.assertEqual(light.attrib, dark.attrib)
            for layer in ["red-layer", "green-layer", "blue-layer"]:
                a = light.find(f"{{{ICON.SVG}}}g[@id='{layer}']")
                b = dark.find(f"{{{ICON.SVG}}}g[@id='{layer}']")
                self.assertEqual(ET.tostring(a), ET.tostring(b))
            self.assertEqual([p.attrib["points"] for p in light.iter(f"{{{ICON.SVG}}}polygon")],
                             [p.attrib["points"] for p in dark.iter(f"{{{ICON.SVG}}}polygon")])
            gradient = dark.find(f".//{{{ICON.SVG}}}linearGradient[@id='top-light']")
            self.assertEqual([s.attrib["stop-color"] for s in gradient], ["#f4f6fa", "#dbe1ea", "#adb9cb"])

    def test_foreground_preserves_source_and_only_adds_twelve_chamfers(self):
        source = ET.parse(ASSETS / "logo.svg").getroot()
        root = ET.parse(ASSETS / "Foveonic.icon/Assets/foreground.svg").getroot()
        foreground = root.find(f"{{{ICON.SVG}}}svg")
        bevel = foreground.find(f".//{{{ICON.SVG}}}g[@id='directional-bevel']")
        self.assertEqual(len(bevel), 12)
        self.assertTrue(all("opacity" not in p.attrib for p in bevel))
        top = foreground.find(f"{{{ICON.SVG}}}g[@id='black-layer']")
        top.remove(bevel)
        def signature(node):
            return (node.tag, node.attrib, (node.text or "").strip(), [signature(c) for c in node])
        self.assertEqual([signature(c) for c in source], [signature(c) for c in foreground])
        self.assertAlmostEqual(float(foreground.attrib["width"]) / 1024, 0.90)


if __name__ == "__main__":
    unittest.main()
