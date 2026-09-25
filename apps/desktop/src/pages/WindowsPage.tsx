import { useEffect, useState } from 'react';
import { Card, ErrorCard, Skeleton, KV, Badge, useBackend } from './Dashboard';
import { fmtBytes, backend, toBackendError, usbEntryId } from '../lib/sysforge';
import ControlPanel from '../components/ControlPanel';
import type { BackendError, EfiBootState, WindowsChecklist as Checklist, RemovableMedia } from '../types';

type ActionState =
  | { kind: 'idle' }
  | { kind: 'working'; msg: string }
  | { kind: 'ok'; msg: string }
  | { kind: 'err'; error: BackendError };

type Countdown = null | { mode: 'reboot' | 'poweroff'; secs: number };

function daemonCall(method: string, params: Record<string, unknown>) {
  return backend<Record<string, unknown>>('daemon_call', { method, params });
}

/** Contagem regressiva na tela inteira — abortável até o último segundo. */
function CountdownOverlay({ cd, onCancel }: { cd: NonNullable<Countdown>; onCancel: () => void }) {
  return (
    <div className="countdown-overlay">
      <div className="countdown-title">
        {cd.mode === 'reboot'
          ? '⚡ A máquina vai REINICIAR e entrar DIRETO no instalador do Windows 11'
          : '⏻ A máquina vai DESLIGAR. Ao ligar, entra DIRETO no instalador do Windows 11'}
      </div>
      <div className="countdown-num">{cd.secs}</div>
      <div className="countdown-sub">
        BootNext one-shot armado — o firmware boota o pendrive automaticamente. BootOrder intacto.
      </div>
      <button onClick={onCancel}>✖ CANCELAR (abortar agora)</button>
    </div>
  );
}

export default function WindowsPage() {
  const checklist = useBackend<Checklist>('windows_checklist');
  const efi = useBackend<EfiBootState>('get_efi_state');
  const [action, setAction] = useState<ActionState>({ kind: 'idle' });
  const [media, setMedia] = useState<RemovableMedia[]>([]);
  const [edition, setEdition] = useState('pro');
  const [fullWipe, setFullWipe] = useState(false);
  const [unattendReady, setUnattendReady] = useState(false);
  const [cd, setCd] = useState<Countdown>(null);

  const refreshMedia = () => {
    backend<RemovableMedia[]>('list_media', {})
      .then(setMedia)
      .catch(() => setMedia([]));
  };
  useEffect(refreshMedia, []);

  useEffect(() => {
    if (!cd) return;
    if (cd.secs <= 0) {
      const mode = cd.mode;
      setCd(null);
      fire(mode);
      return;
    }
    const t = setTimeout(() => setCd({ ...cd, secs: cd.secs - 1 }), 1000);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cd]);

  async function fire(mode: 'reboot' | 'poweroff') {
    try {
      await daemonCall(mode === 'reboot' ? 'v1.system.reboot' : 'v1.system.poweroff', { confirm: true });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  /** SEQUÊNCIA AUTOMÁTICA COMPLETA: privilégio → BootNext(USB) → desligar/reiniciar.
   *  A máquina volta DIRETO no instalador — sem nenhuma etapa manual. */
  async function launchInstaller(mode: 'reboot' | 'poweroff') {
    const verb = mode === 'reboot' ? 'REINICIAR AGORA' : 'DESLIGAR AGORA';
    if (!window.confirm(
      `${verb} e entrar direto no instalador do Windows 11?\n\n` +
      'Isto vai: (1) armazenar BootNext one-shot apontando pro pendrive; ' +
      '(2) ' + (mode === 'reboot' ? 'reiniciar' : 'desligar') + ' a máquina.\n' +
      'O BootOrder fica INTACTO — se o pendrive não bootar, o boot volta ao normal sozinho.\n\nProsseguir?',
    )) return;
    setAction({ kind: 'working', msg: 'Solicitando privilégio (polkit pode pedir sua senha)…' });
    try {
      await backend('ensure_system_daemon', {});
      const state = await backend<EfiBootState>('get_efi_state', {});
      const usb = usbEntryId(state);
      if (!usb) {
        setAction({
          kind: 'err',
          error: {
            code: 'SF-BOOT-006',
            message: 'Nenhuma entrada USB ATIVA no firmware para auto-detectar.',
            recommendation: 'Conecte o pendrive (Ventoy com a ISO dentro) e tente de novo — o firmware só ativa a entrada removível com mídia presente.',
          },
        });
        return;
      }
      setAction({ kind: 'working', msg: `Armando BootNext → ${usb} (one-shot)…` });
      const r = await daemonCall('v1.boot.set_next', { entry_id: usb, confirm: true });
      setAction({
        kind: 'ok',
        msg: `BootNext armado → ${usb}. ${mode === 'reboot' ? 'Reiniciando' : 'Desligando'}…`,
      });
      const snap = typeof r['snapshot_path'] === 'string' ? ` · snapshot: ${r['snapshot_path']}` : '';
      setAction({ kind: 'ok', msg: `BootNext armado → ${usb}${snap}` });
      setCd({ mode, secs: 5 });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  async function generateUnattend() {
    setAction({ kind: 'working', msg: 'Gerando autounattend.xml…' });
    try {
      const r = await backend<{ path: string; on_ventoy: boolean }>('unattend_save', {
        edition,
        full_wipe: fullWipe,
      });
      setUnattendReady(true);
      setAction({
        kind: 'ok',
        msg: `autounattend.xml salvo em ${r.path}${r.on_ventoy ? ' (raiz do Ventoy — o instalador do Windows acha sozinho)' : ' — copie para a RAIZ do pendrive antes do boot'}`,
      });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  const usbEntry = efi.state === 'ok' ? usbEntryId(efi.data) : undefined;
  const hasIso = checklist.state === 'ok' && checklist.data.isos.length > 0;
  const ventoyReady = media.some((m) => m.is_ventoy);
  const bootNextArmed = efi.state === 'ok' && !!efi.data.boot_next;

  return (
    <div className="page">
      {cd && <CountdownOverlay cd={cd} onCancel={() => { setCd(null); setAction({ kind: 'idle' }); }} />}

      <header className="page-head">
        <h1>Instalar Sistema Operacional</h1>
        <p className="page-sub">
          Sequência automática: preparar mídia → respostas (autounattend) → BootNext → desligar/reiniciar.
          Ao voltar, a máquina entra DIRETO no instalador — sem nenhuma etapa manual.
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
          <Card title="Sequência automática" tag="pipeline">
            <div className="pipeline">
              <div className={`pipe-step ${ventoyReady ? 'st-ok' : media.length > 0 ? 'st-warn' : 'st-fail'}`}>
                <span className="pipe-icon">🔌</span>
                <strong>1 · Pendrive Ventoy</strong>
                <span>
                  {ventoyReady
                    ? 'detectado e pronto — a ISO vai pra dentro dele'
                    : media.length > 0
                      ? 'pendrive conectado (sem Ventoy — use Ventoy para boot garantido)'
                      : 'conecte um pendrive (≥ 8 GiB) com Ventoy'}
                </span>
              </div>
              <div className={`pipe-step ${hasIso ? 'st-ok' : 'st-fail'}`}>
                <span className="pipe-icon">💾</span>
                <strong>2 · ISO do sistema</strong>
                <span>
                  {hasIso
                    ? checklist.data.isos.map((i) => i.name).join(', ')
                    : 'baixe a ISO oficial para ~/Downloads (link no checklist abaixo)'}
                </span>
              </div>
              <div className={`pipe-step ${unattendReady ? 'st-ok' : 'st-wait'}`}>
                <span className="pipe-icon">📜</span>
                <strong>3 · Autounattend</strong>
                <span>
                  {unattendReady
                    ? 'gerado — o instalador responde tudo sozinho'
                    : 'gerar abaixo (respostas + bypass de hardware antigo)'}
                </span>
              </div>
              <div className={`pipe-step ${bootNextArmed ? 'st-ok' : 'st-wait'}`}>
                <span className="pipe-icon">⚡</span>
                <strong>4 · Boot automático</strong>
                <span>
                  {bootNextArmed
                    ? `BootNext armado (${efi.state === 'ok' && efi.data.boot_next}) — one-shot, BootOrder intacto`
                    : 'armar e desligar/reiniciar com os botões abaixo'}
                </span>
              </div>
            </div>
            <div className="btn-row">
              <button
                className="btn-primary"
                disabled={!ventoyReady || !hasIso}
                onClick={() => launchInstaller('reboot')}
              >
                ⚡ Reiniciar agora → instalador
              </button>
              <button
                className="btn-off"
                disabled={!ventoyReady || !hasIso}
                onClick={() => launchInstaller('poweroff')}
              >
                ⏻ Desligar (ao ligar, instala)
              </button>
            </div>
            {(!ventoyReady || !hasIso) && (
              <p className="foot-note">
                Os botões ativam quando o pendrive Ventoy estiver conectado e a ISO estiver em ~/Downloads.
              </p>
            )}
            {action.kind === 'working' && <p className="action working">⠿ {action.msg}</p>}
            {action.kind === 'ok' && <p className="action ok">✔ {action.msg}</p>}
            {action.kind === 'err' && (
              <p className="action err">
                ✖ <strong>{action.error.code}</strong> {action.error.message}
                {action.error.recommendation ? <span className="dim"> — {action.error.recommendation}</span> : null}
              </p>
            )}
          </Card>

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
                      if (!e.target.checked || window.confirm(
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

          {efi.state === 'ok' && <ControlPanel efi={efi.data} showEntrySelector />}
        </>
      )}
    </div>
  );
}
