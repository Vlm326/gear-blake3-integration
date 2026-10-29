#!/usr/bin/env python3
"""Reproduce commands.md on deterministic, uncompressed kernel archives."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
MODES = ['baseline', 'fused', 'chunking_only', 'blake3_only']
EVENTS = {
    'basic': ['cycles:u', 'instructions:u', 'cache-references:u', 'cache-misses:u'],
    'l1': ['L1-dcache-loads:u', 'L1-dcache-load-misses:u'],
    'dtlb': ['dTLB-loads:u', 'dTLB-load-misses:u'],
    'l2': ['cycles:u', 'l2_cache_misses_from_dc_misses:u', 'l2_fill_pending.l2_fill_busy:u'],
    'l3': ['l3_lookup_state.all_l3_req_typs', 'l3_comb_clstr_state.request_miss'],
}
ENV = dict(LC_ALL='C', BLAKE3_BATCH='8192', MIN_CHUNK_SIZE='4096',
           AVG_CHUNK_SIZE='10240', MAX_CHUNK_SIZE='65536')


def sha(path):
    with path.open('rb') as f:
        return hashlib.file_digest(f, 'sha256').hexdigest()


def parse_report(path):
    if not path.exists():
        return {}
    text = path.read_text()
    values = {}
    for line in text.splitlines():
        m = re.match(r'\s*([\d,]+)\s+(\S+)(.*)', line)
        if m:
            spread = re.search(r'\+\-\s*([\d.]+)%', m[3])
            values[m[2]] = (float(m[1].replace(',', '')), float(spread[1]) if spread else None)
    m = re.search(r'([\d.]+)\s*\+\-\s*([\d.]+)\s+seconds time elapsed', text)
    if m:
        values['time'] = (float(m[1]), float(m[2]))
    return values


def table(folder, size, iterations, repeats):
    data = {mode: {g: parse_report(folder / f'{mode}_{g}.txt') for g in EVENTS}
            for mode in MODES}
    rows = []
    labels = ['Общие', 'L1', 'DTLB', 'L2', 'L3, вся система']
    names = ['Такты CPU', 'Инструкции', 'Cache references', 'Cache misses',
             'Чтения', 'Промахи чтения', 'Loads (generic)', 'Промахи (generic)',
             'Такты CPU', 'Промахи после L1D', 'Циклы с незавершёнными запросами',
             'Запросы', 'Промахи']
    ni = 0
    for label, (group, events) in zip(labels, EVENTS.items()):
        for event in events:
            rows.append((f'{label}: {names[ni]}', group, event, None))
            ni += 1
        rows.append((f'{label}: Время, с', group, 'time', None))
    derived = [
        ('IPC (общий замер)', 'basic', 'instructions:u', 'cycles:u', 1),
        ('Generic cache miss rate, %', 'basic', 'cache-misses:u', 'cache-references:u', 100),
        ('L1: доля промахов чтения, %', 'l1', EVENTS['l1'][1], EVENTS['l1'][0], 100),
        ('L2: доля циклов с запросами в полёте, %', 'l2', EVENTS['l2'][2], 'cycles:u', 100),
        ('L3: доля промахов всей системы, %', 'l3', EVENTS['l3'][1], EVENTS['l3'][0], 100),
    ]
    for name, event in [('Такты', 'cycles:u'), ('Инструкции', 'instructions:u')]:
        derived.append((f'{name} на байт (общий замер)', 'basic', event, None, 1 / (size * iterations)))
    for group, event in [('basic', EVENTS['basic'][3]), ('l1', EVENTS['l1'][1]),
                         ('dtlb', EVENTS['dtlb'][1]), ('l2', EVENTS['l2'][1]), ('l3', EVENTS['l3'][1])]:
        derived.append((f'{group}: промахи/MiB входа', group, event, None, 1048576 / (size * iterations)))
    rows.extend((name, g, a, (b, factor)) for name, g, a, b, factor in derived)

    def get(mode, group, key, formula):
        item = data[mode][group].get(key)
        if item is None:
            return None, '—'
        value, spread = item
        if formula:
            denom, factor = formula
            d = data[mode][group].get(denom, (0,))[0] if denom else 1
            if not d:
                return None, '—'
            value = value / d * factor
            return value, f'{value:,.3f}'
        text = f'{value:.6f}' if key == 'time' else f'{value:,.0f}'
        if spread is not None:
            text += f' ± {spread:.6f} с' if key == 'time' else f' ± {spread:.2f}%'
        return value, text

    lines = [f'# Кэш-профиль: {folder.name}', '',
             f'Вход: несжатый tar, {size:,} байт; {iterations} итераций, {repeats} повторов; CPU 3.',
             'min/avg/max = 4096/10240/65536; batch = 8192.', '',
             'Дельта = (fused / baseline − 1) × 100%; для долей также относительная.',
             'Счётчики — средние perf; ± у счётчиков — относительный разброс perf, у времени — абсолютный.',
             'Производные показатели рассчитаны по средним. — означает отсутствие данных.', '']
    for modes in [MODES[:2], MODES[2:]]:
        lines += [f'## {" / ".join(modes)}', '',
                  '| Метрика | ' + ' | '.join(modes) + (' | Дельта |' if modes == MODES[:2] else ' |'),
                  '| --- | ---: | ---: |' + (' ---: |' if modes == MODES[:2] else '')]
        for name, group, key, formula in rows:
            items = [get(m, group, key, formula) for m in modes]
            line = '| ' + name + ' | ' + ' | '.join(x[1] for x in items) + ' |'
            if modes == MODES[:2]:
                a, b = (x[0] for x in items)
                line += f' {(b / a - 1) * 100:+.2f}% |' if a and b is not None else ' — |'
            lines.append(line)
        lines.append('')
    a = data['baseline']['basic'].get('time', (0,))[0]
    b = data['fused']['basic'].get('time', (0,))[0]
    if b:
        lines += [f'Ускорение fused по времени общего замера: **{a / b:.3f}×**.', '']
    lines += ['Источники: одноимённые `<режим>_<набор>.txt` в этом каталоге.',
              'Каждый набор событий измерен отдельным запуском. Измеряется весь процесс, включая чтение tar,',
              'а для blake3_only — загрузку и проверку карты. Кэши между итерациями не очищаются.',
              'Tar включает заголовки и заполнение; чанки могут пересекать границы файлов.',
              'L3 включает фон всей системы; нормирование на MiB его не исключает.',
              'Generic-события зависят от CPU. Baseline сохраняет чанки, fused обрабатывает потоком.',
              'Chunking_only включает копирование и аллокации; blake3_only хеширует срезы.',
              'Времена отдельных этапов нельзя складывать как точное разложение baseline.']
    (folder / 'table.md').write_text('\n'.join(lines) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--iterations', type=int, default=5)
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--report', type=Path, help='Regenerate tables for an existing run')
    args = parser.parse_args()
    os.chdir(ROOT)
    if args.report:
        meta = json.loads((args.report / 'metadata.json').read_text())
        for k, v in meta['datasets'].items():
            table(args.report / k, v['bytes'], meta['iterations'], meta['repeats'])
        readme = args.report / 'README.md'
        if readme.exists():
            lines = readme.read_text().splitlines()
            pending = []
            for line in lines:
                if any('/' + mode + '_l3:' in line for mode in MODES):
                    kernel, rest = line.split('/', 1)
                    mode = rest.split('_l3:', 1)[0]
                    report = parse_report(args.report / kernel / f'{mode}_l3.txt')
                    if all(e in report for e in [*EVENTS['l3'], 'time']):
                        continue
                pending.append(line)
            readme.write_text('\n'.join(pending) + '\n')
        return
    if min(args.iterations, args.repeats) < 1:
        parser.error('iterations and repeats must be positive')
    run = ROOT / 'artifacts/cache_info/runs' / (datetime.datetime.now().strftime('%Y-%m-%d_%H-%M-%S') + '_kernels')
    run.mkdir()
    print(f'RUN={run}', flush=True)
    env = os.environ | ENV
    meta = dict(iterations=args.iterations, repeats=args.repeats, environment=ENV, datasets={},
                binary_sha256=sha(ROOT / 'target/release/hash_combination'))
    for name, cmd in {'commit': ['git', 'rev-parse', 'HEAD'], 'status': ['git', 'status', '--short'],
                      'machine': ['lscpu'], 'perf': ['perf', '--version'], 'rust': ['rustc', '--version']}.items():
        meta[name] = subprocess.check_output(cmd, text=True)
    (run / 'working-tree.patch').write_bytes(subprocess.check_output(['git', 'diff']))
    meta['source_sha256'] = {str(p): sha(p) for p in sorted(Path('src').rglob('*.rs'))}
    l3 = subprocess.run(['sudo', '-n', 'perf', 'stat', '-a', '-e', '{' + ','.join(EVENTS['l3']) + '}', '--', 'true'], capture_output=True, text=True, env=env)
    (run / 'l3_preflight.log').write_text(l3.stdout + l3.stderr)
    commands = []
    pending = ['#!/usr/bin/env bash', 'set -euo pipefail', 'cd ' + shlex.quote(str(ROOT))]
    issues = []
    for kernel in sorted((ROOT / 'kernels').glob('linux-*')):
        folder = run / kernel.name
        folder.mkdir()
        archive = ROOT / 'artifacts/inputs' / (kernel.name + '-' + run.name + '.tar')
        archive.parent.mkdir(exist_ok=True)
        cmd = ['tar', '--sort=name', '--format=gnu', '--mtime=@0', '--owner=0', '--group=0', '--numeric-owner', '-cf', str(archive), '-C', str(kernel.parent), kernel.name]
        subprocess.run(cmd, check=True)
        commands.append(shlex.join(cmd))
        size = archive.stat().st_size
        chunkmap = folder / 'chunks.tsv'
        cmd = ['./target/release/hash_combination', '--mode', 'prepare_chunks', '--chunk-map', str(chunkmap), str(archive)]
        subprocess.run(cmd, check=True, env=env)
        commands.append(shlex.join(['env', *[f'{k}={v}' for k, v in ENV.items()], *cmd]))
        if not meta['datasets']:
            pilot = ['taskset', '-c', '3', './target/release/hash_combination', '--mode', 'fused', '--iterations', str(args.iterations), str(archive)]
            started = time.monotonic()
            subprocess.run(pilot, check=True, env=env, stdout=subprocess.DEVNULL)
            elapsed = time.monotonic() - started
            meta['pilot_seconds'] = elapsed
            print(f'Pilot: {elapsed:.3f} seconds', flush=True)
            if elapsed < 0.5:
                raise RuntimeError('Pilot too short; increase iterations before full series')
        meta['datasets'][kernel.name] = dict(archive=str(archive), bytes=size, sha256=sha(archive), map_sha256=sha(chunkmap))
        (run / 'metadata.json').write_text(json.dumps(meta, indent=2))
        for group, events in EVENTS.items():
            for mode in MODES:
                cmd = ['env', *[f'{k}={v}' for k, v in ENV.items()], 'perf', 'stat']
                if group == 'l3':
                    cmd = ['sudo', '-n', *cmd, '-a']
                cmd += ['-r', str(args.repeats), '-e', '{' + ','.join(events) + '}', '-o', str(folder / f'{mode}_{group}.txt'), '--', 'taskset', '-c', '3', './target/release/hash_combination', '--mode', mode, '--iterations', str(args.iterations)]
                if mode == 'blake3_only':
                    cmd += ['--chunk-map', str(chunkmap)]
                cmd.append(str(archive))
                if group == 'l3' and l3.returncode:
                    pending.append(shlex.join([x for x in cmd if x != '-n']))
                    issues.append(f'{kernel.name}/{mode}_{group}: требуется sudo; см. l3_preflight.log')
                    continue
                print(f'{kernel.name}: {mode}/{group}', flush=True)
                commands.append(shlex.join(cmd))
                with (folder / f'{mode}_{group}.log').open('w') as log:
                    result = subprocess.run(cmd, stdout=log, stderr=log)
                report = folder / f'{mode}_{group}.txt'
                text = report.read_text() if report.exists() else ''
                if result.returncode or any(x not in parse_report(report) for x in [*events, 'time']) or re.search(r'<not (?:counted|supported)>', text):
                    raise RuntimeError(f'Invalid measurement: {report}')
                percentages = re.findall(r'\(([\d.]+)%\)', text)
                if any(float(p) < 99 for p in percentages):
                    issues.append(f'{report.name}: время работы счётчиков ниже 99%')
        table(folder, size, args.iterations, args.repeats)
        (run / 'commands.sh').write_text('#!/usr/bin/env bash\nset -euo pipefail\ncd ' + shlex.quote(str(ROOT)) + '\n' + '\n'.join(commands) + '\n')
        (run / 'run_l3.sh').write_text('\n'.join(pending) + '\n')
        (run / 'README.md').write_text('# Профилирование ядер Linux\n\n' + '\n'.join(f'- [{k}]({k}/table.md)' for k in meta['datasets']) + '\n\nПроблемы / пропуски:\n' + ('\n'.join(issues) or 'Нет.') + '\n\nПосле выполнения run_l3.sh обновить таблицы:\n```bash\npython3 scripts/benchmark_kernel_cache.py --report ' + shlex.quote(str(run)) + '\n```\n')


if __name__ == '__main__':
    main()
