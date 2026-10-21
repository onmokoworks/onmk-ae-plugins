"""Summarize explicitly indexed evidence; counts are not certification.

Reads only referenced summary.json files, not render trees or plugin sources.
Optional integrity checks open only the exact raw outputs named by hashed cases.
Optional parameter inspection opens exact passed-case fixtures and per-run baselines.
Different binary hashes remain separate; older evidence is never relabeled current.
"""
import argparse
import hashlib
from collections import Counter
import json
from pathlib import Path


def check_artifacts(run_directory, rows):
    checked = 0
    problems = []
    for row in rows:
        expected = row.get('sha256')
        if not expected:
            continue  # Metadata-only checks do not have an image artifact.
        case = row.get('case')
        if not isinstance(case, str) or not case or '/' in case or '\\' in case or case in ['.', '..']:
            problems.append(dict(case=case, error='invalid_artifact_case'))
            continue
        path = (run_directory/'renders'/case/'final'/'output.bin').resolve()
        if not path.is_relative_to(run_directory.resolve()):
            problems.append(dict(case=case, error='artifact_outside_run'))
            continue
        try:
            digest = hashlib.sha256()
            with path.open('rb') as stream:
                for block in iter(lambda: stream.read(1024 * 1024), b''):
                    digest.update(block)
            checked += 1
            if digest.hexdigest() != expected:
                problems.append(dict(case=case, error='artifact_hash_mismatch'))
        except OSError as error:
            problems.append(dict(case=case, error='artifact_unreadable', detail=str(error)))
    return dict(checked=checked, problems=problems)


def parameter_stimuli(run_directory, rows):
    """Configured differences, not proof that controls affect output."""
    problems=[]
    observed={}
    try:
        baseline=json.loads((run_directory/'parameters.json').read_text(encoding='utf-8-sig'))
        defaults={(p.get('slot',i+1),p['name'],p['kind']):p for i,p in enumerate(baseline)}
        if len(defaults)!=len(baseline):
            raise ValueError('duplicate baseline parameter identities')
    except (OSError, ValueError, KeyError, TypeError) as error:
        return dict(observed={}, problems=[dict(error='baseline_unavailable',detail=str(error))])
    for row in rows:
        if row.get('status')!='passed' or not row.get('sha256'):
            continue
        case=row.get('case')
        if not isinstance(case,str) or not case or '/' in case or '\\' in case or case in ['.','..']:
            problems.append(dict(case=case,error='invalid_fixture_case')); continue
        path=(run_directory/f'{case}.json').resolve()
        if not path.is_relative_to(run_directory.resolve()):
            problems.append(dict(case=case,error='fixture_outside_run')); continue
        try:
            fixture=json.loads(path.read_text(encoding='utf-8-sig'))
            parameters=fixture['parameters']
            changes=[]
            for i,p in enumerate(parameters):
                key=(p.get('slot',i+1),p['name'],p['kind'])
                original=defaults[key]
                for field in ['value','layer_path','color','components']:
                    if p.get(field)!=original.get(field):
                        changes.append((p['name'],dict(case=case,slot=key[0],kind=p['kind'],field=field,
                            baseline=original.get(field),configured=p.get(field),oracle=row.get('oracle'))))
            for name,change in changes:
                observed.setdefault(name,[]).append(change)
        except (OSError,ValueError,KeyError,TypeError) as error:
            problems.append(dict(case=case,error='fixture_unreadable',detail=str(error)))
    return dict(observed=observed,problems=problems)


def summarize(repository, index, verify_artifacts=False, inspect_parameters=False):
    base = (repository / index['audit_directory']).resolve()
    targets = {}
    depth_coverage = {}
    stimulus_evidence = {}
    parameter_evidence = {}
    for target, primary in index['targets'].items():
        evidence = []
        run_rows = {}
        for name in dict.fromkeys([primary] + index.get('supplemental_evidence', {}).get(target, [])):
            path = (base / name / 'summary.json').resolve()
            if not path.is_relative_to(base):
                raise ValueError(f'Evidence escapes audit directory: {name}')
            if not path.is_file():
                evidence.append({'name': name, 'state': 'missing'})
                continue
            data = json.loads(path.read_text(encoding='utf-8-sig'))
            rows = data.get('results', [])
            run_rows[name] = rows
            names = Counter(row.get('case', '<missing>') for row in rows)
            integrity = check_artifacts(path.parent, rows) if verify_artifacts else None
            evidence.append(dict(name=name, state='read', plugin_sha256=data.get('plugin_sha256'),
                artifact_integrity=integrity,
                runner_sha256=data.get('runner_sha256'), harness_sha256=data.get('harness_sha256'),
                statuses=dict(Counter(row.get('status', 'missing') for row in rows)),
                render_depths=sorted({row['depth'] for row in rows if 'depth' in row}),
                passed_depths=sorted({row['depth'] for row in rows
                                     if 'depth' in row and row.get('status') == 'passed'}),
                oracles=dict(Counter(row.get('oracle', 'unspecified') for row in rows)),
                duplicate_cases=[case for case, count in names.items() if count > 1],
                unresolved=[dict(case=row.get('case'), status=row.get('status'), error=row.get('error'))
                            for row in rows if row.get('status') != 'passed'],
                limitations=data.get('limitations', [])))
        targets[target] = evidence
        primary_hash = evidence[0].get('plugin_sha256')
        for position, run in enumerate(evidence):
            run['role'] = 'primary' if position == 0 else 'supplemental'
            tested_hash = run.get('plugin_sha256')
            run['matches_primary_build'] = (tested_hash == primary_hash
                if tested_hash and primary_hash else None)
            problems = []
            if run['state'] == 'missing':
                problems.append('missing_summary')
            else:
                if not tested_hash:
                    problems.append('missing_tested_build_hash')
                if not sum(run['statuses'].values()):
                    problems.append('empty_results')
                if run['duplicate_cases']:
                    problems.append('duplicate_case_names')
                if run['unresolved']:
                    problems.append('nonpassing_results')
                if run['artifact_integrity'] and run['artifact_integrity']['problems']:
                    problems.append('artifact_integrity_failure')
            run['evidence_problems'] = problems
        declared = index.get('declared_depths', {}).get(target)
        eligible = [run for run in evidence if run['matches_primary_build'] is True
                    and not run['evidence_problems']]
        if inspect_parameters:
            parameter_evidence[target]=dict(runs={run['name']:parameter_stimuli(base/run['name'],run_rows[run['name']]) for run in eligible},
                note='Fixture differences from inspected defaults only; not output response, redraw, same-instance invalidation or all-control coverage. Fixture metadata is not cryptographically tied to rendered pixels.')
        passed = sorted({depth for run in eligible for depth in run.get('passed_depths', [])})
        depth_coverage[target] = dict(declared=declared, clean_primary_build_passed_depths=passed,
            declared_without_clean_pass=sorted(set(declared) - set(passed)) if declared is not None else None,
            note='Depth presence only, not numerical precision or runtime capability proof.')
        observed = {key: [] for key in ['black_input', 'transparent_input',
            'thin_vertical', 'thin_horizontal', 'odd_dimensions', 'repeat_agreement']}
        for run in eligible:
            for row in run_rows[run['name']]:
                case = row.get('case', '')
                categories = []
                if row.get('sha256') and row.get('status') == 'passed':
                    width, height = row.get('width'), row.get('height')
                    if case in {f'black-{w}x{h}' for w, h in
                                [(1,1), (1,31), (17,1), (17,31), (63,65), (321,181), (127,257)]}:
                        categories.append('black_input')
                    if case == 'transparent':
                        categories.append('transparent_input')
                    if isinstance(width, int) and isinstance(height, int) and width > 0 and height > 0:
                        if width == 1 and height > 1:
                            categories.append('thin_vertical')
                        if height == 1 and width > 1:
                            categories.append('thin_horizontal')
                        if width > 1 and height > 1 and width % 2 and height % 2:
                            categories.append('odd_dimensions')
                if case in ['repeat_agreement', 'repeat_and_smart_agreement'] and row.get('status') == 'passed':
                    categories.append('repeat_agreement')
                for category in categories:
                    observed[category].append(dict(run=run['name'], **{key: row.get(key)
                        for key in ['case', 'width', 'height', 'depth', 'path', 'oracle']}))
        stimulus_evidence[target] = dict(observed=observed,
            without_indexed_observation=[key for key, rows in observed.items() if not rows],
            note='Recorded stimuli only; black/transparent labels are standard runner cases. '
                 'Repeat agreement is not same-instance resize, temporal correctness or MFR proof. '
                 'Absent observations can exist in other direct tests; this is not a failure verdict.')
    return dict(schema='onmk.quality_evidence.v1', targets=targets,
                depth_coverage=depth_coverage,
                stimulus_evidence=stimulus_evidence,
                parameter_evidence=parameter_evidence,
                target_count=len(targets),
                note='Evidence inventory only. Repeated cases across runs are not unique coverage. '
                     'Depth presence does not prove precision; finite/shape checks do not prove visual correctness. '
                     'Binary hashes identify tested builds, not current source or installed plugins.',
                open_requirements=index.get('open_requirements', []))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--index', type=Path, default=Path(__file__).with_name('quality_coverage.json'))
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--verify-artifacts', action='store_true', help='Hash exact indexed raw outputs; no recursive file scan')
    parser.add_argument('--inspect-parameters', action='store_true', help='Read exact passed-case fixtures and run parameter baselines; no recursive scan')
    args = parser.parse_args()
    repository = Path(__file__).resolve().parents[1]
    report = summarize(repository, json.loads(args.index.read_text(encoding='utf-8-sig')), args.verify_artifacts, args.inspect_parameters)
    args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding='utf-8')
    for target, runs in report['targets'].items():
        statuses = Counter()
        for run in runs:
            statuses.update(run.get('statuses', {}))
        print(target, json.dumps(dict(statuses)),
              'different_build_runs=', sum(r['matches_primary_build'] is False for r in runs),
              'evidence_problem_runs=', sum(bool(r['evidence_problems']) for r in runs))
