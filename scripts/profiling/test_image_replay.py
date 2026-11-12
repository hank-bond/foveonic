"""Check fresh-image replay and its checked-scenario handoff."""

import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from scenarios import compare_scenario_images


class ImageReplayTests(unittest.TestCase):
    def setUp(self):
        quiet = patch('image_replay.require_quiet')
        quiet.start()
        self.addCleanup(quiet.stop)

    def test_image_comparison_matches_final_frames_and_validates_identities(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            evidence=root/'evidence'
            evidence.mkdir()
            binary=root/'renderer'
            binary.write_bytes(b'renderer')
            geometry={'extent':[2,1],'layout':'Rgb32Float','space':'test','linear_white':1,
                      'view_baked':False,'view_extent':[2,1],'view_crop':[0,0,2,1],
                      'view_rotation':0,'endianness':'little'}
            photos=[]
            captures=[]
            inputs=[]
            for index in range(2):
                source=str(root/f'{index}.x3f')
                settings={'exposure':index+.5,'exposure_auto':False}
                snapshot={'settings':settings,'working_scale':8}
                photo={'source':source,'task':index+10,'revision':4,'settings':settings,'scale_divisor':8}
                capture={**geometry,'source':source,'task':index+10,'revision':4,'settings':snapshot,
                         'file':str(evidence/f'image-{index:02}.f32')}
                Path(capture['file']).write_bytes(bytes(range(24)))
                photos.append(photo)
                captures.append(capture)
                inputs.append({'source':f'original-{index}.x3f','staged_source':source,
                               'settings':{'exposure':index,'exposure_auto':False}})
            native={'passed':True,'hardware':{'adapter':'GPU'},'image_captures':captures,
                    'steps':[{'action':'initial-selection','photos':[{'source':photo['source'],
                              'settings':{'exposure':index,'exposure_auto':False}} for index,photo in enumerate(photos)]},
                             {'action':'slider-drag','photos':photos}]}
            drag={'pointer':'/exposure','paths':[photo['source'] for photo in photos],
                  'initial':[0,1], 'frames':[{'values':[.5,1.5]}], 'errors':[]}
            (evidence/'slider-drag.json').write_text(json.dumps(drag))
            def fresh(command,**_kwargs):
                snapshot=json.loads(Path(command[command.index('--settings-snapshot')+1]).read_text())
                goldens=Path(command[command.index('--goldens')+1])
                goldens.mkdir(exist_ok=True)
                (goldens/(Path(command[1]).stem+'.f32')).write_bytes(bytes(range(24)))
                rows=[('session',{'settings':snapshot['settings'],'adapter':'GPU'}),
                      ('processing',{'effective_scale_divisor':8}),('validation',geometry)]
                return SimpleNamespace(returncode=0,stderr='',stdout='\n'.join(json.dumps({'event':event,'data':data}) for event,data in rows))
            with patch('image_replay.subprocess.run',side_effect=fresh):
                checks=compare_scenario_images(native,inputs,binary,root)
            self.assertEqual([item['outcome'] for item in checks],['passed','passed'])
            self.assertEqual([item['source'] for item in checks],['original-0.x3f','original-1.x3f'])
            native['browser_journey']={'expected':{'exposures':[.5,1.5]}}
            native['steps'][-1]['action']='browser-journey'
            with patch('image_replay.subprocess.run',side_effect=fresh):
                self.assertTrue(all(check['outcome']=='passed' for check in compare_scenario_images(native,inputs,binary,root)))
            native.pop('browser_journey')
            native['steps'][-1]['action']='slider-drag'
            crop={'bounds':[.125,.125,.875,.875],'aspect':'unconstrained','flipped':False}
            rotation={'turns':1,'degrees':2.}
            photos[0]['settings']['crop']=crop
            photos[0]['settings']['rotation']=rotation
            captures[0]['settings']['settings']={**photos[0]['settings'],'crop':{'bounds':[0,0,1,1]}}
            with self.assertRaisesRegex(ValueError,'captured image crop'):
                compare_scenario_images(native,inputs,binary,root)
            captures[0]['settings']['settings']=photos[0]['settings']
            initial = native['steps'][0]['photos'][0]['settings']
            initial['exposure']=99
            with self.assertRaisesRegex(ValueError,'initial /exposure does not match'):
                compare_scenario_images(native,inputs,binary,root)
            initial['exposure']=0
            captures[0]['task']=99
            with self.assertRaisesRegex(ValueError,'not the settled final frame'):
                compare_scenario_images(native,inputs,binary,root)
            captures[0]['task']=10
            captures[0]['settings']['settings']['exposure']=9
            photos[0]['settings']={'exposure':.5,'exposure_auto':False}
            with self.assertRaisesRegex(ValueError,'do not match the checked edited property'):
                compare_scenario_images(native,inputs,binary,root)



if __name__ == '__main__':
    unittest.main()
