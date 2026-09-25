import { useEffect, useState } from 'react';
import { Card, ErrorCard, Skeleton, KV, Badge } from './Dashboard';
import { fmtBytes, backend, toBackendError } from '../lib/sysforge';
import type { BackendError } from '../types';

interface BackupDir { name: string; path: string; size_bytes: number; exists: boolean }
interface BackupTarget { mount: string; device: string; fs: string; free_bytes: number }

type ActionState =
  | { kind: 'idle' }
  | { kind: 'working'; msg: string }
  | { kind: 'ok'; msg: string }
  | { kind: 'err'; error: BackendError };

export default function BackupPage() {
  const [dirs, setDirs] = useState<BackupDir[]>([]);
  const [targets, setTargets] = useState<BackupTarget[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [target, setTarget] = useState('');
  const [loading, setLoading] = useState(true);
  const [action, setAction] = useState<ActionState>({ kind: 'idle' });

  useEffect(() => {
    backend<{ dirs: BackupDir[]; targets: BackupTarget[] }>('backup_state', {})
      .then((s) => {
        setDirs(s.dirs);
        setTargets(s.targets);
        setSelected(new Set(s.dirs.map((d) => d.name)));
        if (s.targets[0]) setTarget(s.targets[0].mount);
      })
      .catch((e) => setAction({ kind: 'err', error: toBackendError(e) }))
      .finally(() => setLoading(false));
  }, []);

  const totalSel = dirs.filter((d) => selected.has(d.name)).reduce((a, d) => a + d.size_bytes, 0);
  const tgt = targets.find((t) => t.mount === target);

  async function estimate() {
    if (!target) return;
    setAction({ kind: 'working', msg: 'Medindo pastas (real, byte a byte)…' });
    try {
      const e = await backend<{ total_bytes: number; free_bytes: number; fits: boolean }>(
        'backup_estimate', { dirs: [...selected], target },
      );
      if (e.fits) {
        setAction({ kind: 'ok', msg: `Cabe: ${fmtBytes(e.total_bytes)} de dados em ${fmtBytes(e.free_bytes)} livres no destino` });
      } else {
        setAction({
          kind: 'err',
          error: { code: 'SF-BACKUP-001', message: `NÃO CABE: ${fmtBytes(e.total_bytes)} > ${fmtBytes(e.free_bytes)} livres — escolha menos pastas ou outro destino` },
        });
      }
    } catch (e2) {
      setAction({ kind: 'err', error: toBackendError(e2) });
    }
  }

  async function run() {
    if (!window.confirm(
      `Copiar ${[...selected].join(', ')} para ${target}/sysforge-backup/ ?\n\n` +
      'rsync sem --delete (nunca apaga nada no destino). Antes de trocar de sistema, este é o passo que protege seus arquivos.',
    )) return;
    setAction({ kind: 'working', msg: 'Copiando com rsync (pode demorar conforme o tamanho)…' });
    try {
      const r = await backend<{ dest: string }>('backup_run', { dirs: [...selected], target });
      setAction({ kind: 'ok', msg: `Backup concluído em ${r.dest}` });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>Backup</h1>
        <p className="page-sub">
          Antes de trocar de sistema: copie seus arquivos para um disco externo. Sem root —
          cópia de usuário para usuário, rsync sem <code>--delete</code> (o destino nunca perde nada).
        </p>
      </header>
      {loading && <Skeleton />}
      {!loading && targets.length === 0 && (
        <Card title="Destino do backup" tag="não encontrado">
          <p className="dim">
            Nenhum disco externo montado foi detectado (procurado em /media e /run/media).
            Conecte um disco USB ou uma partição de dados montada — depois abra esta página de novo.
          </p>
        </Card>
      )}
      {!loading && targets.length > 0 && (
        <div className="grid">
          <Card title="O que salvar (pastas suas)" tag="$HOME">
            {dirs.map((d) => (
              <div className="form-row check" key={d.name}>
                <label style={{ display: 'flex', gap: 10, alignItems: 'center', minWidth: 0 }}>
                  <input
                    type="checkbox"
                    checked={selected.has(d.name)}
                    onChange={(e) => {
                      const s = new Set(selected);
                      if (e.target.checked) s.add(d.name); else s.delete(d.name);
                      setSelected(s);
                    }}
                  />
                  <span style={{ flex: 1 }}>{d.name}</span>
                  <span className="dim">{fmtBytes(d.size_bytes)}</span>
                </label>
              </div>
            ))}
            {dirs.length === 0 && <p className="dim">Nenhuma pasta clássica encontrada no seu $HOME.</p>}
            <div className="bar">
              <div className="bar-label">
                <span>total selecionado</span>
                <span>{fmtBytes(totalSel)}</span>
              </div>
              <div className="bar-track"><div className="bar-fill" style={{ width: '100%' }} /></div>
            </div>
          </Card>

          <Card title="Para onde salvar" tag="montagens externas">
            {targets.map((t) => (
              <KV
                key={t.mount}
                k={t.mount}
                v={
                  <label>
                    <input
                      type="radio"
                      name="tgt"
                      checked={target === t.mount}
                      onChange={() => setTarget(t.mount)}
                    />{' '}
                    {t.device} · {t.fs} · {fmtBytes(t.free_bytes)} livres
                  </label>
                }
              />
            ))}
            <div className="btn-row">
              <button onClick={() => estimate()}>Medir (estimativa real)</button>
              <button className="btn-primary" disabled={selected.size === 0 || !target} onClick={() => run()}>
                ▶ Executar backup agora
              </button>
            </div>
            {action.kind === 'working' && <p className="action working">⠿ {action.msg}</p>}
            {action.kind === 'ok' && <p className="action ok">✔ {action.msg}</p>}
            {action.kind === 'err' && (
              <p className="action err">✖ <strong>{action.error.code}</strong> {action.error.message}</p>
            )}
          </Card>
        </div>
      )}
    </div>
  );
}
