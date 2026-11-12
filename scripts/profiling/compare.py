"""Compare matched, uncontended native Off reports with explicit optional regression budgets."""

import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
from pathlib import Path

from trace_analysis import distribution
from interaction_latency import geometry_identity, value_hash


def baseline_samples(report):
    if report.get('mode') != 'off':
        raise ValueError('comparison requires mode Off reports')
    trials=report.get('trials',[])
    if (report.get('passed') is not True or report.get('timing_valid') is not True or
            not trials or len(trials)!=report.get('trials_requested')):
        raise ValueError('comparison requires complete behavioral success and valid timing for every trial')
    key=report.get('workload_key')
    if (not key or report.get('workload_consistent') is not True or
            value_hash(key['descriptor'])!=key['sha256'] or
            key['descriptor'].get('first_current_complete') is not True or
            geometry_identity(key['descriptor'].get('geometry',{})) is None or
            not key['descriptor'].get('geometry_complete') or
            not key['descriptor'].get('hardware',{}).get('application')):
        raise ValueError('report lacks a complete, consistent matched workload key')
    samples=defaultdict(list)
    action_counts=None
    for trial in trials:
        contention=trial.get('contention',{})
        if (trial.get('mode')!='off' or trial.get('input_kind')!='baseline' or
                trial.get('passed') is not True or trial.get('timing_valid') is not True or
                contention.get('checked') is not True or contention.get('uncontended') is not True):
            raise ValueError('each Off trial must have successful behavior and checked, uncontended timing')
        if trial.get('workload_key')!=key:
            raise ValueError('report contains mixed workload keys')
        counts=Counter()
        for step in trial['steps']:
            action=step['action'];value=step.get('baseline_elapsed_ms')
            if not isinstance(value,(int,float)) or isinstance(value,bool) or not math.isfinite(value) or value<0:
                raise ValueError('every baseline step needs a finite nonnegative elapsed duration')
            counts[action]+=1;samples[action].append(value)
        if not counts or (action_counts is not None and counts!=action_counts):
            raise ValueError('trials contain incomplete or inconsistent action groups')
        action_counts=counts
    return samples,action_counts


def compare_reports(before, after, default_budget=None, action_budgets=None):
    budgets=action_budgets or {}
    if any(not math.isfinite(value) or value<0 for value in [*budgets.values(),*([default_budget] if default_budget is not None else [])]):
        raise ValueError('regression budgets must be finite nonnegative percentages')
    left,left_counts=baseline_samples(before);right,right_counts=baseline_samples(after)
    if before['workload_key']!=after['workload_key']:
        raise ValueError('baseline and candidate have different workload keys')
    if left_counts!=right_counts:
        raise ValueError('baseline and candidate have different action groups')
    if budgets.keys()-left.keys():
        raise ValueError('a supplied action budget does not match a measured action')
    actions=[]
    decisions=[]
    for action in sorted(left):
        baseline=distribution(left[action]);candidate=distribution(right[action])
        budget=budgets.get(action,default_budget)
        metrics={}
        for name in ('median','p95'):
            old,new=baseline[name],candidate[name]
            within=None if budget is None else new<=old*(1+budget/100)
            if within is not None:decisions.append(within)
            metrics[name]={'before_ms':old,'after_ms':new,'delta_ms':new-old,
                           'delta_percent':100*(new-old)/old if old else (0. if new==0 else None),
                           'within_budget':within}
        actions.append({'action':action,'before_samples':baseline['count'],'after_samples':candidate['count'],
                        'max_regression_percent':budget,**metrics})
    return {'schema_version':1,'input_kind':'native-off-comparison','workload_key':before['workload_key'],
            'passed':all(decisions) if decisions else None,'actions':actions,
            'artifacts':{'before':[trial['artifacts'] for trial in before['trials']],
                         'after':[trial['artifacts'] for trial in after['trials']]},
            'limits':['Budgets are caller-supplied. Without a budget this command reports deltas without a pass/fail threshold.',
                      'Median and nearest-rank p95 describe these samples, not statistical significance.',
                      'Workload matching includes recorded host, application GPU, window/viewport geometry and pixel density.',
                      'Controlled-delivery journeys include intentional harness waiting and are not natural rendering throughput measurements.']}


def report_identity(path):
    return {'path':str(path.resolve()),'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before',type=Path)
    parser.add_argument('after',type=Path)
    parser.add_argument('--max-regression-percent',type=float,help='explicit budget applied to median and p95 of every action')
    parser.add_argument('--budget',action='append',default=[],metavar='ACTION=PERCENT',help='override the budget for one measured action')
    parser.add_argument('--output',type=Path,help='new comparison JSON file; stdout is always emitted')
    args=parser.parse_args()
    try:
        budgets={}
        for item in args.budget:
            action,separator,value=item.partition('=')
            if not separator or not action or action in budgets:raise ValueError('budgets must be distinct ACTION=PERCENT assignments')
            budgets[action]=float(value)
        result=compare_reports(json.loads(args.before.read_text()),json.loads(args.after.read_text()),args.max_regression_percent,budgets)
        result['reports']={'before':report_identity(args.before),'after':report_identity(args.after)}
        text=json.dumps(result,indent=2,allow_nan=False)+'\n'
        if args.output:
            with args.output.open('x') as output:output.write(text)
    except (ValueError,KeyError,OSError) as error:
        parser.exit(1,str(error)+'\n')
    print(text,end='')
    if result['passed'] is False:raise SystemExit(1)


if __name__=='__main__':main()
