"""Check independent scenario edits before fresh-image replay."""

import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from scenarios import compare_scenario_images
from scenario_checks import validate_choice_recipes


class ScenarioCheckTests(unittest.TestCase):
    def setUp(self):
        quiet = patch('image_replay.require_quiet')
        quiet.start()
        self.addCleanup(quiet.stop)

    def test_choice_journey_recipes_and_independent_final_evidence(self):
        cases=[{'settings':{'white_balance':wb,'tone_enabled':True,'tone':{'contrast':0.25},
                            'exposure':index,'exposure_auto':False}}
               for index,wb in enumerate(('Sunlight','Incandescent'))]
        validate_choice_recipes(cases)
        for change in ('same', 'disabled', 'neutral', 'missing'):
            bad=json.loads(json.dumps(cases))
            if change=='same':bad[1]['settings']['white_balance']='Sunlight'
            elif change=='disabled':bad[1]['settings']['tone_enabled']=False
            elif change=='neutral':bad[1]['settings']['tone']['contrast']=0
            else:bad[1].pop('settings')
            with self.assertRaises(ValueError):validate_choice_recipes(bad)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            evidence=root/'evidence'
            evidence.mkdir()
            geometry={'extent':[2,1],'layout':'Rgb32Float','space':'test','linear_white':1,
                      'view_baked':False,'view_extent':[2,1],'view_crop':[0,0,2,1],
                      'view_rotation':0,'endianness':'little'}
            inputs=[]
            initial=[]
            photos=[]
            captures=[]
            for index,case in enumerate(cases):
                source=str(root/f'{index}.x3f')
                start=case['settings']
                final={**start,'white_balance':'Sunlight','tone_enabled':False}
                inputs.append({'source':f'original-{index}.x3f','staged_source':source,'settings':start})
                initial.append({'source':source,'settings':dict(start)})
                photos.append({'source':source,'task':index+10,'revision':4,'settings':dict(final),'scale_divisor':8})
                capture={**geometry,'source':source,'task':index+10,'revision':4,
                         'settings':{'settings':dict(final),'working_scale':8},'file':str(evidence/f'image-{index:02}.f32')}
                Path(capture['file']).write_bytes(bytes(range(24)))
                captures.append(capture)
            events=[{'observation':'mixed White Balance'},{'action':'choose Sunlight'},
                    {'action':'disable Tone'},{'observation':'Tone unchecked and final images current'}]
            native={'passed':True,'hardware':{'adapter':'GPU'},'image_captures':captures,
                    'steps':[{'action':'initial-selection','photos':initial},{'action':'choice-journey','photos':photos}],
                    'choices':{'events':events}}
            def fresh(command,**_kwargs):
                snapshot=json.loads(Path(command[command.index('--settings-snapshot')+1]).read_text())
                goldens=Path(command[command.index('--goldens')+1])
                goldens.mkdir(exist_ok=True)
                (goldens/(Path(command[1]).stem+'.f32')).write_bytes(bytes(range(24)))
                rows=[('session',{'settings':snapshot['settings'],'adapter':'GPU'}),
                      ('processing',{'effective_scale_divisor':8}),('validation',geometry)]
                return SimpleNamespace(returncode=0,stderr='',stdout='\n'.join(json.dumps({'event':event,'data':data}) for event,data in rows))
            with patch('image_replay.subprocess.run',side_effect=fresh):
                self.assertTrue(all(check['outcome']=='passed' for check in compare_scenario_images(native,inputs,root/'renderer',root)))
                photos[1]['settings']['white_balance']='Incandescent'
                with self.assertRaisesRegex(ValueError,'independent expectation'):compare_scenario_images(native,inputs,root/'renderer',root)
                photos[1]['settings']['white_balance']='Sunlight'
                photos[0]['settings']['exposure']=0.5
                with self.assertRaisesRegex(ValueError,'independent expectation'):compare_scenario_images(native,inputs,root/'renderer',root)
                photos[0]['settings']['exposure']=0
                captures[1]['settings']['settings']['tone_enabled']=True
                with self.assertRaisesRegex(ValueError,'captured image choices'):compare_scenario_images(native,inputs,root/'renderer',root)
                captures[1]['settings']['settings']['tone_enabled']=False
                initial[1]['settings']['white_balance']='Sunlight'
                with self.assertRaisesRegex(ValueError,'initial choices'):compare_scenario_images(native,inputs,root/'renderer',root)
                initial[1]['settings']['white_balance']='Incandescent'
                native['choices']['events']=events[:-1]
                with self.assertRaisesRegex(ValueError,'input evidence'):compare_scenario_images(native,inputs,root/'renderer',root)


    def test_numeric_slider_checks_declared_values_and_effective_settings(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            evidence=root/'evidence'
            evidence.mkdir()
            source=str(root/'photo.x3f')
            initial={'tone':{'contrast':0.1},'exposure_auto':True}
            final={'tone':{'contrast':0.2},'exposure_auto':True}
            photo={'source':source,'task':1,'revision':2,'settings':final,'scale_divisor':8}
            native={'passed':True,'hardware':{'adapter':'GPU'},
                    'steps':[{'action':'initial-selection','photos':[{'source':source,'settings':initial}]},
                             {'action':'slider-drag','photos':[photo]}],
                    'image_captures':[{'source':source,'task':1,'revision':2,
                                       'settings':{'settings':final,'working_scale':8}}]}
            inputs=[{'source':'original','staged_source':source,'settings':{'tone':{'contrast':0.1}}}]
            drag={'pointer':'/tone/contrast','paths':[source],'initial':[0.1],
                  'frames':[{'values':[0.2]}],'errors':[]}
            path=evidence/'slider-drag.json'
            path.write_text(json.dumps(drag))
            initial['tone']['contrast']=0.3
            with self.assertRaisesRegex(ValueError,'initial /tone/contrast'):
                compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')
            initial['tone']['contrast']=0.1
            native['image_captures'][0]['settings']['settings']={'tone':{'contrast':0.4},'exposure_auto':True}
            with self.assertRaisesRegex(ValueError,'checked edited property'):
                compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')
            native['image_captures'][0]['settings']['settings']=final
            geometry={'extent':[1,1],'layout':'Rgb32Float','space':'test','linear_white':1,
                      'view_baked':False,'view_extent':[1,1],'view_crop':[0,0,1,1],
                      'view_rotation':0,'endianness':'little'}
            capture=native['image_captures'][0]
            capture.update(geometry, file=str(evidence/'image-00.f32'))
            Path(capture['file']).write_bytes(bytes(12))
            def fresh(command,**_kwargs):
                snapshot=json.loads(Path(command[command.index('--settings-snapshot')+1]).read_text())
                self.assertTrue(snapshot['settings']['exposure_auto'])
                goldens=Path(command[command.index('--goldens')+1])
                goldens.mkdir()
                (goldens/(Path(source).stem+'.f32')).write_bytes(bytes(12))
                rows=[('session',{'settings':snapshot['settings'],'adapter':'GPU'}),
                      ('processing',{'effective_scale_divisor':8}),('validation',geometry)]
                return SimpleNamespace(returncode=0,stderr='',stdout='\n'.join(json.dumps({'event':event,'data':data}) for event,data in rows))
            with patch('image_replay.subprocess.run',side_effect=fresh):
                self.assertEqual(compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')[0]['outcome'],'passed')
            drag['frames'][-1]['values']=[0.4]
            path.write_text(json.dumps(drag))
            with self.assertRaisesRegex(ValueError,'slider drag values differ'):
                compare_scenario_images(native,inputs,root/'renderer',root,'/tone/contrast')



if __name__ == '__main__':
    unittest.main()
