"""Check isolated test inputs preserve original bytes and own their recipe writes."""

import json
from pathlib import Path
import tempfile
import unittest

from inputs import stage_inputs


class InputTests(unittest.TestCase):
    def test_inputs_are_hardlinked_and_recipe_writes_are_owned(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            source=root/'source.x3f'
            source.write_bytes(b'RAW sample bytes')
            cases=[{'source':str(source),'settings':{'exposure':.25}}]
            records=stage_inputs(cases,root/'inputs')
            staged=Path(records[0]['staged_source'])
            self.assertEqual(source.stat().st_ino,staged.stat().st_ino)
            self.assertEqual(source.read_bytes(),b'RAW sample bytes')
            recipe=json.loads((staged.parent/'foveonic'/(staged.name+'.foveonic.json')).read_text())
            self.assertEqual(recipe,{'format':'foveonic-editing','version':1,'settings':{'exposure':.25}})
            self.assertFalse((root/'foveonic').exists())


if __name__ == '__main__':
    unittest.main()
