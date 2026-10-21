import json
import hashlib
from pathlib import Path
import tempfile
import unittest
from quality_evidence import summarize, parameter_stimuli


class EvidenceTests(unittest.TestCase):
    def test_same_named_controls_are_distinguished_by_slot(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            baseline=[dict(slot=i,name='Color',kind='color',color=[0,0,0]) for i in [2,4]]
            (root/'parameters.json').write_text(json.dumps(baseline),encoding='utf-8')
            baseline[1]['color']=[1,2,3]
            (root/'case.json').write_text(json.dumps(dict(parameters=baseline)),encoding='utf-8')
            result=parameter_stimuli(root,[dict(case='case',status='passed',sha256='hash')])
            self.assertEqual(result['problems'],[])
            self.assertEqual(result['observed']['Color'][0]['slot'],4)
            self.assertEqual(result['observed']['Color'][0]['field'],'color')

    def test_parameter_inventory_is_configuration_not_response(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            baseline=[dict(name='Radius',kind='slider',value=1),dict(name='Map',kind='layer')]
            (root/'parameters.json').write_text(json.dumps(baseline),encoding='utf-8')
            configured=[dict(name='Radius',kind='slider',value=4),dict(name='Map',kind='layer',layer_path='map.png')]
            (root/'changed.json').write_text(json.dumps(dict(parameters=configured)),encoding='utf-8')
            rows=[dict(case='changed',status='passed',sha256='hash',oracle='shape_only'),
                  dict(case='failed',status='failed_unclassified',sha256='hash'),
                  dict(case='aggregate',status='passed'),
                  dict(case='../escape',status='passed',sha256='hash'),
                  dict(case='missing',status='passed',sha256='hash')]
            result=parameter_stimuli(root,rows)
            self.assertEqual(set(result['observed']),{'Radius','Map'})
            self.assertEqual(result['observed']['Radius'][0]['configured'],4)
            self.assertEqual(result['observed']['Radius'][0]['baseline'],1)
            self.assertEqual(result['observed']['Map'][0]['field'],'layer_path')
            self.assertEqual([p['error'] for p in result['problems']],['invalid_fixture_case','fixture_unreadable'])

    def test_stimuli_require_clean_matching_build_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = b'pixels'
            digest = hashlib.sha256(raw).hexdigest()
            for name, build, case, width, height, status in [
                    ('primary','new','black-1x31',1,31,'passed'),
                    ('same','new','pattern-17x1',17,1,'passed'),
                    ('old','old','transparent',63,65,'passed'),
                    ('failed','new','transparent',63,65,'failed_unclassified'),
                    ('corrupt','new','transparent',63,65,'passed')]:
                run = root/'audit'/name
                output = run/'renders'/case/'final'/'output.bin'
                output.parent.mkdir(parents=True)
                output.write_bytes(b'bad' if name == 'corrupt' else raw)
                rows = [dict(case=case,width=width,height=height,status=status,
                             depth='argb8',path='classic',sha256=digest,oracle='shape_only')]
                if name == 'primary':
                    rows.append(dict(case='repeat_agreement',status='passed'))
                (run/'summary.json').write_text(json.dumps(dict(plugin_sha256=build,results=rows)),encoding='utf-8')
            index = dict(audit_directory='audit',targets={'p':'primary'},
                         supplemental_evidence={'p':['same','old','failed','corrupt']})
            result = summarize(root,index,True)['stimulus_evidence']['p']
            self.assertEqual(result['without_indexed_observation'],['transparent_input','odd_dimensions'])
            self.assertEqual(result['observed']['thin_horizontal'][0]['run'],'same')
            self.assertEqual(result['observed']['black_input'][0]['oracle'],'shape_only')
            self.assertEqual(len(result['observed']['repeat_agreement']),1)
            self.assertIn('not same-instance resize',result['note'])

    def test_corrupt_or_missing_artifacts_cannot_supply_clean_depth_evidence(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            run = root/'audit'/'run'
            output = run/'renders'/'image'/'final'/'output.bin'
            output.parent.mkdir(parents=True)
            raw = b'original pixels'
            output.write_bytes(raw)
            row = dict(case='image',status='passed',depth='argb16',sha256=hashlib.sha256(raw).hexdigest())
            (run/'summary.json').write_text(json.dumps(dict(plugin_sha256='build',results=[row])),encoding='utf-8')
            index = dict(audit_directory='audit',targets={'p':'run'},declared_depths={'p':['argb16']})
            clean = summarize(root,index,True)
            self.assertEqual(clean['depth_coverage']['p']['clean_primary_build_passed_depths'],['argb16'])
            for replacement in [b'corrupt', None]:
                if replacement is None: output.unlink()
                else: output.write_bytes(replacement)
                report = summarize(root,index,True)
                self.assertEqual(report['depth_coverage']['p']['clean_primary_build_passed_depths'],[])
                self.assertIn('artifact_integrity_failure',report['targets']['p'][0]['evidence_problems'])
            row['case'] = '../outside'
            (run/'summary.json').write_text(json.dumps(dict(plugin_sha256='build',results=[row])),encoding='utf-8')
            self.assertEqual(summarize(root,index,True)['targets']['p'][0]['artifact_integrity']['problems'][0]['error'], 'invalid_artifact_case')

    def test_failed_and_old_depths_do_not_complete_declared_coverage(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, build, depth, status in [('primary','new','argb8','passed'),
                    ('old','old','argb16','passed'), ('failed','new','argb32f','failed_unclassified')]:
                run = root/'audit'/name
                run.mkdir(parents=True)
                (run/'summary.json').write_text(json.dumps(dict(plugin_sha256=build,
                    results=[dict(case=name,depth=depth,status=status)])),encoding='utf-8')
            index = dict(audit_directory='audit',targets={'p':'primary'},
                supplemental_evidence={'p':['old','failed']},
                declared_depths={'p':['argb8','argb16','argb32f']})
            coverage = summarize(root,index)['depth_coverage']['p']
            self.assertEqual(coverage['clean_primary_build_passed_depths'],['argb8'])
            self.assertEqual(coverage['declared_without_clean_pass'],['argb16','argb32f'])

    def test_missing_failed_and_duplicate_records_are_not_passes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            run = root/'audit'/'run'
            run.mkdir(parents=True)
            (run/'summary.json').write_text(json.dumps(dict(plugin_sha256='tested-build', runner_sha256='runner', harness_sha256='harness', results=[
                dict(case='a', status='passed', depth='argb8'),
                dict(case='a', status='running'),
                dict(case='b', status='failed_unclassified', error='expected error missing')
            ])), encoding='utf-8')
            index = dict(audit_directory='audit', targets={'plugin':'run'},
                         supplemental_evidence={'plugin':['run','missing']})
            runs = summarize(root,index)['targets']['plugin']
            self.assertEqual(len(runs),2)
            self.assertEqual(runs[0]['plugin_sha256'],'tested-build')
            self.assertEqual(runs[0]['runner_sha256'],'runner')
            self.assertEqual(runs[0]['harness_sha256'],'harness')
            self.assertEqual(runs[0]['duplicate_cases'],['a'])
            self.assertEqual(len(runs[0]['unresolved']),2)
            self.assertEqual(runs[1]['state'],'missing')

    def test_evidence_must_stay_inside_audit_directory(self):
        with self.assertRaises(ValueError):
            summarize(Path.cwd(),dict(audit_directory='audit',targets={'plugin':'../outside'}))

    def test_old_build_is_not_evidence_for_primary_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name, build in [('primary','new'),('same','new'),('old','old')]:
                run = root/'audit'/name
                run.mkdir(parents=True)
                (run/'summary.json').write_text(json.dumps(dict(plugin_sha256=build,
                    results=[dict(case='normal',status='passed')])), encoding='utf-8')
            runs = summarize(root,dict(audit_directory='audit',targets={'p':'primary'},
                supplemental_evidence={'p':['same','old','missing']}))['targets']['p']
            self.assertEqual([r['matches_primary_build'] for r in runs], [True,True,False,None])
            self.assertEqual(runs[0]['role'], 'primary')
            self.assertEqual(runs[2]['role'], 'supplemental')
            self.assertEqual(runs[2]['plugin_sha256'], 'old')
            self.assertEqual(runs[3]['evidence_problems'], ['missing_summary'])

    def test_empty_or_unidentified_results_cannot_appear_clean(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            run = root/'audit'/'empty'
            run.mkdir(parents=True)
            (run/'summary.json').write_text('{}',encoding='utf-8')
            result = summarize(root,dict(audit_directory='audit',targets={'p':'empty'}))['targets']['p'][0]
            self.assertEqual(result['matches_primary_build'],None)
            self.assertEqual(result['evidence_problems'],['missing_tested_build_hash','empty_results'])


if __name__=='__main__':
    unittest.main()
