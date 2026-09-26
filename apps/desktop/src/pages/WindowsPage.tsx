import { useEffect, useState } from 'react';
import { Card, ErrorCard, Skeleton, KV, Badge, useBackend } from './Dashboard';
import { fmtBytes, backend, toBackendError, usbEntryId } from '../lib/sysforge';
import ControlPanel from '../components/ControlPanel';
import type { BackendError, EfiBootState, DiskBootReadiness, WindowsChecklist as Checklist, RemovableMedia } from '../types';

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
  const [smallStick, setSmallStick] = useState('');
  const [smallEdition, setSmallEdition] = useState(4);
  const [fullWipe, setFullWipe] = useState(false);
  const [unattendReady, setUnattendReady] = useState(false);
  const [cd, setCd] = useState<Countdown>(null);
  const [disk, setDisk] = useState<DiskBootReadiness | null>(null);
  const [diskBusy, setDiskBusy] = useState(false);

  // Sondagem IN-PROCESS (sem daemon, sempre fresca) — atualiza a cada 5s.
  const refreshDisk = () => {
    backend<DiskBootReadiness>('get_disk_readiness', {})
      .then(setDisk)
      .catch(() => setDisk(null));
  };
  useEffect(refreshDisk, []);
  useEffect(() => {
    const t = setInterval(refreshDisk, 5000);
    return () => clearInterval(t);
  }, []);

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
  /** LANÇADOR ÚNICO — decide o método NA HORA do clique, com dados frescos:
   *  pendrive Ventoy ativo → USB; senão disco pronto (sem pendrive) → GRUB/wimboot.
   *  Botão nunca fica morto: se nada estiver pronto, o erro aparece aqui embaixo. */
  async function launchInstaller(mode: 'reboot' | 'poweroff') {
    setAction({ kind: 'working', msg: 'Sondando o que está pronto (mídia, firmware, disco)…' });
    try {
      const [mediaNow, efiNow, diskNow] = await Promise.all([
        backend<RemovableMedia[]>('list_media', {}).catch(() => [] as RemovableMedia[]),
        backend<EfiBootState>('get_efi_state', {}).catch(() => null),
        backend<DiskBootReadiness>('get_disk_readiness', {}),
      ]);
      const ventoy = mediaNow.some((m) => m.is_ventoy);
      const usb = efiNow ? usbEntryId(efiNow) : undefined;
      const verb = mode === 'reboot' ? 'REINICIAR AGORA' : 'DESLIGAR AGORA';

      const useUsb = ventoy && !!usb;
      const useDisk = !useUsb && diskNow.ready;

      if (!useUsb && !useDisk) {
        const faltas = [
          ...(diskNow.iso_path ? [] : ['ISO do Windows não encontrada em ~/Downloads']),
          ...diskNow.blockers,
          ...(!ventoy && !usb && !diskNow.ready ? ['nem pendrive Ventoy nem método disco pronto'] : []),
        ];
        setAction({
          kind: 'err',
          error: {
            code: 'SF-BOOT-030',
            message: 'Ainda não há um caminho pronto para instalar.',
            recommendation: faltas.filter((f, i, a) => a.indexOf(f) === i).join(' · ') || 'verifique o checklist abaixo',
          },
        });
        return;
      }

      if (!window.confirm(
        `${verb} e entrar DIRETO no instalador do Windows 11?\n\n` +
        `Método detectado: ${useUsb ? `pendrive Ventoy (${usb} — BootNext one-shot)` : 'SEM PENDRIVE — GRUB carrega o instalador da ISO direto para a RAM (wimboot)'}.\n` +
        'O BootOrder fica INTACTO — se algo falhar, o boot volta ao normal sozinho.\n\nProsseguir?',
      )) return;

      setAction({ kind: 'working', msg: 'Solicitando privilégio (o polkit pode pedir sua senha na tela)…' });
      await backend('ensure_system_daemon', {});

      if (useUsb && usb) {
        setAction({ kind: 'working', msg: `Armando BootNext → ${usb} (one-shot)…` });
        const r = await daemonCall('v1.boot.set_next', { entry_id: usb, confirm: true });
        const snap = typeof r['snapshot_path'] === 'string' ? ` · snapshot: ${r['snapshot_path']}` : '';
        setAction({ kind: 'ok', msg: `BootNext armado → ${usb}${snap}` });
      } else {
        setAction({ kind: 'working', msg: 'Preparando GRUB + wimboot (método sem pendrive)…' });
        const prep = await daemonCall('v1.install.disk_prepare', { confirm: true });
        await daemonCall('v1.install.disk_arm', { confirm: true });
        setAction({
          kind: 'ok',
          msg: `GRUB pronto (ISO: ${prep['iso_path'] ?? '—'}) · próximo boot: DIRETO no instalador (one-shot)`,
        });
      }
      setCd({ mode, secs: 5 });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  /** Pendrive otimizado: mídia de boot direta para pendrive de 4 GB. */
  async function buildSmallUsb() {
    if (!smallStick) return;
    const planR = await daemonCall('v1.install.small_usb_plan', { device: smallStick, edition_index: smallEdition })
      .catch((e) => { setAction({ kind: 'err', error: toBackendError(e) }); return null; });
    if (!planR) return;
    if (!planR['fits']) {
      setAction({ kind: 'err', error: { code: 'SF-DISK-021', message: `Sem espaço: pendrive ${fmtBytes(planR['stick_size_bytes'] as number)} < mídia otimizada estimada (~4,1 GB)` } });
      return;
    }
    if (!window.confirm(
      `APAGAR TODO O CONTEÚDO DE ${smallStick} e montar a mídia de instalação direta (${planR['edition_name']})?\n` +
      'Esta mídia é padrão Microsoft: a firmware boota direto. Seu computador NÃO é tocado.',
    )) return;
    setAction({ kind: 'working', msg: 'Montando mídia otimizada (export comprime ~30-60 min na 1ª vez, depois usa cache)…' });
    try {
      const r = await daemonCall('v1.install.small_usb_build', {
        device: smallStick, edition_index: smallEdition, confirm: true,
        autounattend: '/home/llinux/Downloads/autounattend.xml',
      });
      setAction({ kind: 'ok', msg: `✔ Pendrive pronto (${r['edition']}). Agora clique em ⚡ Reiniciar e instalar — a firmware boota o pendrive.` });
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
            <p className="foot-note">
              Método que será usado:{' '}
              {ventoyReady && usbEntry ? (
                <Badge kind="ok">pendrive Ventoy ({usbEntry}) — BootNext</Badge>
              ) : disk?.ready ? (
                <Badge kind="ok">sem pendrive — Disco + Nuvem (GRUB/wimboot na RAM)</Badge>
              ) : (
                <Badge kind="warn">sondando… (ISO/pendrive/máquina)</Badge>
              )}
            </p>
            <div className="btn-row">
              <button className="btn-primary" onClick={() => launchInstaller('reboot')}>
                ⚡ Reiniciar e instalar agora
              </button>
              <button className="btn-off" onClick={() => launchInstaller('poweroff')}>
                ⏻ Desligar (ao ligar, instala)
              </button>
            </div>
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

          <Card title="Sem pendrive — método Disco + Nuvem" tag="wimboot/GRUB">
            <p className="dim">
              Para quem não tem pendrive de 8 GB: a ISO fica no seu disco e o GRUB (que já boota
              sua máquina) carrega o instalador do Windows direto para a RAM via <code>wimboot</code>.
              Nenhuma partição é criada, nenhuma alteração no layout — 100% reversível.
            </p>
            {disk === null ? (
              <p className="foot-note">Sondagem do método disco precisa do daemon — clique em "Solicitar privilégio" no painel abaixo e volte aqui.</p>
            ) : (
              <>
                <div className="kv"><span className="kv-k">ISO em disco</span><span className="kv-v">{disk.iso_path ?? <Badge kind="err">ausente — baixe para ~/Downloads</Badge>}</span></div>
                <div className="kv"><span className="kv-k">Secure Boot</span><span className="kv-v">{disk.secure_boot_off ? <Badge kind="ok">desligado (ok p/ wimboot)</Badge> : <Badge kind="err">ligado — desligue na BIOS</Badge>}</span></div>
                <div className="kv"><span className="kv-k">GRUB</span><span className="kv-v">{disk.grub_present ? <Badge kind="ok">presente</Badge> : <Badge kind="err">ausente</Badge>}</span></div>
                <div className="kv"><span className="kv-k">RAM livre</span><span className="kv-v">{disk.ram_ok ? <Badge kind="ok">{disk.ram_available_mb} MiB (precisa {disk.ram_needed_mb})</Badge> : <Badge kind="err">{`${disk.ram_available_mb} MiB < ${disk.ram_needed_mb} MiB`}</Badge>}</span></div>
                <div className="kv"><span className="kv-k">wimboot</span><span className="kv-v">{disk.wimboot_present ? <Badge kind="ok">instalado</Badge> : <Badge kind="info">será baixado (iPXE oficial) na preparação</Badge>}</span></div>
                    {disk.ready ? (
                  <p className="action ok">✔ pronto — os botões de cima usam este método sozinho</p>
                ) : (
                  <p className="action err">✖ pendências: {disk.blockers.join(' · ')}</p>
                )}
              </>
            )}
          </Card>

          <Card title="Pendrive otimizado — boot direto (para pendrive de 4 GB)" tag="100% padrão Microsoft">
            <p className="dim">
              A ISO oficial não cabe num pendrive de 4 GB — mas uma mídia de boot <b>direta</b> com UMA edição
              comprimida no formato oficial (<code>install.esd</code>) cabe com folga. O resultado é um pendrive
              FAT32 padrão: a firmware boota nativamente, como na mídia da Microsoft. <b className="danger">Isto APAGA o pendrive inteiro.</b>
            </p>
            <div className="form-row">
              <label>Pendrive</label>
              <select value={smallStick} onChange={(e) => setSmallStick(e.target.value)}>
                <option value="">— conecte um pendrive ≥ 4 GB —</option>
                {media.filter((m) => m.size_bytes > 3_000_000_000).map((m) => (
                  <option key={m.name} value={'/dev/' + m.name}>
                    /dev/{m.name} · {fmtBytes(m.size_bytes)}
                  </option>
                ))}
              </select>
            </div>
            <div className="form-row">
              <label>Edição</label>
              <select value={smallEdition} onChange={(e) => setSmallEdition(Number(e.target.value))}>
                <option value={1}>Windows 11 Home</option>
                <option value={4}>Windows 11 Pro</option>
              </select>
            </div>
            <div className="btn-row">
              <button className="danger" disabled={!smallStick} onClick={() => buildSmallUsb()}>
                🔨 Preparar pendrive otimizado (apaga o pendrive)
              </button>
            </div>
          </Card>

          {efi.state === 'ok' && <ControlPanel efi={efi.data} showEntrySelector />}
        </>
      )}
    </div>
  );
}
