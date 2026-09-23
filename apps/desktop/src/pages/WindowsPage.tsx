import { useEffect, useState } from 'react';
import { Card, ErrorCard, Skeleton, KV, Badge, useBackend } from './Dashboard';
import { fmtBytes, backend, toBackendError } from '../lib/sysforge';
import ControlPanel from '../components/ControlPanel';
import type { BackendError, WindowsChecklist as Checklist, RemovableMedia } from '../types';

type ActionState =
  | { kind: 'idle' }
  | { kind: 'working'; msg: string }
  | { kind: 'ok'; msg: string }
  | { kind: 'err'; error: BackendError };

export default function WindowsPage() {
  const checklist = useBackend<Checklist>('windows_checklist');
  const [action, setAction] = useState<ActionState>({ kind: 'idle' });
  const [media, setMedia] = useState<RemovableMedia[]>([]);
  const [edition, setEdition] = useState('pro');
  const [fullWipe, setFullWipe] = useState(false);

  const refreshMedia = () => {
    backend<RemovableMedia[]>('list_media', {})
      .then(setMedia)
      .catch(() => setMedia([]));
  };
  useEffect(refreshMedia, []);

  async function callDaemon(method: string, params: Record<string, unknown>, doing: string, done: string) {
    setAction({ kind: 'working', msg: doing });
    try {
      const result = await backend<Record<string, unknown>>(method === '__ensure'
        ? 'ensure_system_daemon'
        : 'daemon_call', method === '__ensure' ? {} : { method, params });
      setAction({ kind: 'ok', msg: done + (result && typeof result === 'object' ? detailSuffix(result) : '') });
      return result;
    } catch (e) {
      const err = toBackendError(e);
      setAction({ kind: 'err', error: err });
      return null;
    }
  }

  function detailSuffix(r: Record<string, unknown>): string {
    if (typeof r['snapshot_path'] === 'string') return ` · snapshot: ${r['snapshot_path']}`;
    if (typeof r['socket'] === 'string') return ` · ${r['socket']}`;
    return '';
  }

  async function generateUnattend() {
    setAction({ kind: 'working', msg: 'Gerando autounattend.xml…' });
    try {
      const r = await backend<{ path: string; on_ventoy: boolean }>('unattend_save', {
        edition,
        full_wipe: fullWipe,
      });
      setAction({
        kind: 'ok',
        msg: `autounattend.xml salvo em ${r.path}${r.on_ventoy ? ' (raiz do Ventoy — o instalador do Windows acha sozinho)' : ' — copie para a RAIZ do pendrive antes do boot'}`,
      });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  const confirmDialog = (msg: string): boolean => window.confirm(msg);

  return (
    <div className="page">
      <header className="page-head">
        <h1>Instalar Sistema Operacional</h1>
        <p className="page-sub">
          Fluxo real: preparar mídia → armazenar respostas (autounattend) → BootNext → reiniciar.
          Após o reboot, o instalador do sistema assume com tudo pronto.
        </p>
        <div className="target-cards">
          <div className="target-card active">
            <strong>Windows 11</strong>
            <span>Pronto agora — checklist real, autounattend com bypass de hardware antigo (TPM/CPU), BootNext one-shot.</span>
          </div>
          <div className="target-card soon">
            <strong>Ubuntu / Linux Mint</strong>
            <span>Em breve — a engenharia de mídia/checklist já é agnóstica; falta o autoboot por distro.</span>
          </div>
          <div className="target-card soon">
            <strong>Outros sistemas</strong>
            <span>Em breve — Ventoy aceita qualquer ISO: use a cópia manual hoje, o fluxo guiado vem aí.</span>
          </div>
        </div>
      </header>

      {checklist.state === 'loading' && <Skeleton />}
      {checklist.state === 'error' && <ErrorCard where="Checklist" error={checklist.error} />}

      {checklist.state === 'ok' && (
        <>
          <Card title="Diagnóstico da máquina (sondagem real)" tag="checklist">
            <table className="table">
              <thead>
                <tr>
                  <th>Status</th>
                  <th>Item</th>
                  <th>Detalhe</th>
                  <th>Ação</th>
                </tr>
              </thead>
              <tbody>
                {checklist.data.items.map((item) => (
                  <tr key={item.id}>
                    <td>
                      {item.status === 'ok' && <Badge kind="ok">ok</Badge>}
                      {item.status === 'warn' && <Badge kind="warn">aviso</Badge>}
                      {item.status === 'fail' && <Badge kind="err">crítico</Badge>}
                      {item.status === 'info' && <Badge kind="info">info</Badge>}
                    </td>
                    <td>{item.title}</td>
                    <td className="dim">{item.detail}</td>
                    <td className="dim">{item.hint ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="foot-note">
              <strong>Recomendação:</strong> {checklist.data.recommendation}
            </p>
          </Card>

          <div className="grid">
            <Card title="Mídia USB detectada" tag="lsblk rm:1">
              {media.length === 0 ? (
                <p className="dim">Nenhum pendrive conectado. Conecte um (≥ 8 GiB, Ventoy recomendado) e o SYSFORGE faz o resto.</p>
              ) : (
                media.map((m) => (
                  <KV
                    key={m.name}
                    k={`/dev/${m.name}`}
                    v={`${fmtBytes(m.size_bytes)} · ${m.is_ventoy ? <Badge kind="ok">Ventoy {m.mounted_at ?? 'não montado'}</Badge> : m.fstype ?? '—'}`}
                  />
                ))
              )}
              <p className="foot-note">
                {checklist.data.isos.length === 0
                  ? 'Nenhuma ISO > 3 GiB encontrada em ~/Downloads — baixe a oficial: microsoft.com/pt-br/software-download/windows11'
                  : `ISOs: ${checklist.data.isos.map((i) => `${i.name} (${fmtBytes(i.size_bytes)})`).join(', ')}`}
              </p>
            </Card>

            <Card title="Autounattend (respostas da instalação)" tag="xml">
              <div className="form-row">
                <label>Edição</label>
                <select value={edition} onChange={(e) => setEdition(e.target.value)}>
                  <option value="pro">Windows 11 Pro</option>
                  <option value="home">Windows 11 Home</option>
                </select>
              </div>
              <div className="form-row check">
                <label>
                  <input
                    type="checkbox"
                    checked={fullWipe}
                    onChange={(e) => {
                      if (!e.target.checked || confirmDialog(
                          'ATENÇÃO: isto faz o autounattend APAGAR O DISCO 0 INTEIRO durante o setup do Windows — incluindo este Linux e TODOS os arquivos. Sem este modo, o instalador pergunta onde instalar. Confirmar?',
                        )) {
                        setFullWipe(e.target.checked);
                      }
                    }}
                  />
                  <span>
                    Modo apagar disco 0 (instalação totalmente automática){' '}
                    <strong className="danger">— destrutivo</strong>
                  </span>
                </label>
              </div>
              <p className="foot-note">
                O unattend inclui bypass LabConfig (TPM/CPU/SecureBoot/RAM) — hardware antigo instala, com a ressalva de suporte oficial da Microsoft.
              </p>
              <div className="btn-row">
                <button onClick={() => generateUnattend()}>Gerar autounattend.xml</button>
              </div>
            </Card>
          </div>

          <ControlPanel />
        </>
      )}
    </div>
  );
}
