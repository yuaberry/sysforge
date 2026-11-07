import { useEffect, useState } from 'react';
import { Card, ErrorCard, Skeleton, KV, Badge, useBackend } from './Dashboard';
import { fmtBytes, backend, toBackendError } from '../lib/sysforge';
import ControlPanel from '../components/ControlPanel';
import type { BackendError, EfiBootState, DiskBootReadiness, WindowsChecklist as Checklist, RemovableMedia } from '../types';

type ActionState =
  | { kind: 'idle' }
  | { kind: 'working'; msg: string }
  | { kind: 'ok'; msg: string }
  | { kind: 'err'; error: BackendError };

type Countdown = null | { mode: 'reboot'; secs: number };

function daemonCall(method: string, params: Record<string, unknown>) {
  return backend<Record<string, unknown>>('daemon_call', { method, params });
}

/** Contagem regressiva na tela inteira — abortável até o último segundo. */
function CountdownOverlay({ cd, onCancel }: { cd: NonNullable<Countdown>; onCancel: () => void }) {
  return (
    <div className="countdown-overlay">
      <div className="countdown-title">⚡ A máquina vai REINICIAR e entrar DIRETO no instalador do Windows 11</div>
      <div className="countdown-num">{cd.secs}</div>
      <div className="countdown-sub">
        Mídia de boot preparada — a firmware boota o pendrive automaticamente.<br />
        O BootOrder fica INTACTO — se o pendrive não for detectado, o boot volta ao normal sozinho.
      </div>
      <button onClick={onCancel} className="btn-danger">✖ CANCELAR (abortar agora)</button>
    </div>
  );
}

export default function WindowsPage() {
  const checklist = useBackend<Checklist>('windows_checklist');
  const efi = useBackend<EfiBootState>('get_efi_state');
  const [action, setAction] = useState<ActionState>({ kind: 'idle' });
  const [media, setMedia] = useState<RemovableMedia[]>([]);
  const [disk, setDisk] = useState<DiskBootReadiness | null>(null);
  const [cd, setCd] = useState<Countdown>(null);

  // Sondagem in-process (sempre fresca) — atualiza a cada 5s.
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
      setCd(null);
      fireReboot();
      return;
    }
    const t = setTimeout(() => setCd({ ...cd, secs: cd.secs - 1 }), 1000);
    return () => clearTimeout(t);
  }, [cd]);

  async function fireReboot() {
    try {
      await daemonCall('v1.system.reboot', { confirm: true });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  /** Fluxo único e garantido:
   *  1. Detecta ISO em ~/Downloads
   *  2. Detecta pendrive conectado (≥4 GB)
   *  3. Se Ventoy ativo → armar BootNext → reiniciar
   *  4. Se pendrive comum → preparar pendrive otimizado (install.esd) → armar BootNext → reiniciar
   *  5. Se nada pronto → erro honesto dizendo exatamente o que falta
   *
   *  O método "Disco + Nuvem" (wimboot/GRUB) foi REMOVIDO do fluxo de instalação:
   *  ele abre o WinPE mas o setup do Windows não lê partições ext4 — não completa.
   *  O caminho que completa é mídia de boot FAT32 padrão no pendrive.
   */
  async function launchInstaller() {
    setAction({ kind: 'working', msg: 'Sondando máquina (ISO, pendrive, firmware)…' });

    try {
      const [mediaNow, efiNow, diskNow] = await Promise.all([
        backend<RemovableMedia[]>('list_media', {}).catch(() => [] as RemovableMedia[]),
        backend<EfiBootState>('get_efi_state', {}).catch(() => null),
        backend<DiskBootReadiness>('get_disk_readiness', {}),
      ]);

      const usbEntry = efiNow ? efiNow.entries.find(
        (e) => (e.name.toLowerCase().includes('usb') || e.name.toLowerCase().includes('removable')) &&
               (e.device_path ?? '').includes('VenMsg')
      ) : undefined;
      const ventoy = mediaNow.some((m) => m.is_ventoy);
      const stickReady = mediaNow.some((m) => m.size_bytes > 3_000_000_000);
      const isoReady = diskNow.iso_path != null;

      // ---- PASSO 1: tentar caminho pronto (Ventoy ou otimizado já feito) ----
      if (ventoy && usbEntry) {
        // Ventoy já pronto: armar BootNext e reiniciar.
        if (!window.confirm(
          'Reiniciar agora e entrar direto no instalador do Windows 11?\n\n' +
          'Método: pendrive Ventoy detectado — BootNext one-shot será armado.\n' +
          'O BootOrder fica intacto. Se algo falhar, o boot volta ao normal sozinho.\n\nProsseguir?',
        )) return;

        setAction({ kind: 'working', msg: 'Solicitando privilégio (polkit pode pedir sua senha)…' });
        await daemonCall('ensure_system_daemon', {});

        setAction({ kind: 'working', msg: `Armando BootNext → ${usbEntry.id} (one-shot)…` });
        const r = await daemonCall('v1.boot.set_next', { entry_id: usbEntry.id, confirm: true });
        const snap = typeof r === 'object' && r && 'snapshot_path' in r ? ` · snapshot: ${(r as any).snapshot_path}` : '';
        setAction({ kind: 'ok', msg: `BootNext armado → ${usbEntry.id}.${snap}` });
        setCd({ mode: 'reboot', secs: 5 });
        return;
      }

      // ---- PASSO 2: pendrive conectado mas não pronto → preparar otimizado ----
      if (stickReady && isoReady && diskNow.ready) {
        const stick = mediaNow.find((m) => m.size_bytes > 3_000_000_000)!;
        if (!window.confirm(
          `Preparar pendrive ${stick.name} (${fmtBytes(stick.size_bytes)}) e reiniciar para instalar Windows 11?\n\n` +
          'Isto APAGA TODO O CONTEÚDO do pendrive e cria uma mídia de boot direta.\n' +
          'Seu computador NÃO é tocado até você reiniciar com o pendrive conectado.\n\nProsseguir?',
        )) return;

        setAction({ kind: 'working', msg: 'Solicitando privilégio (polkit pode pedir sua senha)…' });
        await daemonCall('ensure_system_daemon', {});

        setAction({ kind: 'working', msg: 'Preparando pendrive otimizado (export da edição Pro — pode levar minutos)…' });
        const buildR = await daemonCall('v1.install.small_usb_build', {
          device: stick.path, edition_index: 4, confirm: true,
          autounattend: '/home/llinux/Downloads/autounattend.xml',
        });
        if (typeof buildR === 'object' && buildR && !(buildR as any).ok === false && !(buildR as any).error) {
          setAction({ kind: 'ok', msg: `Pendrive ${stick.name} pronto — mídia de boot direta criada.` });
        } else {
          throw new Error(typeof buildR === 'object' && buildR ? (buildR as any).error?.message || 'falha ao preparar' : 'falha desconhecida');
        }

        // Após preparar, tentar detectar entrada USB no firmware para armar BootNext.
        const efiAfter = await backend<EfiBootState>('get_efi_state', {}).catch(() => null);
        const usbAfter = efiAfter?.entries.find(
          (e) => (e.name.toLowerCase().includes('usb') || e.name.toLowerCase().includes('removable')) &&
                 (e.device_path ?? '').includes('VenMsg')
        );

        if (usbAfter) {
          setAction({ kind: 'working', msg: `Armando BootNext → ${usbAfter.id} (one-shot)…` });
          const r2 = await daemonCall('v1.boot.set_next', { entry_id: usbAfter.id, confirm: true });
          setAction({ kind: 'ok', msg: `BootNext armado → ${usbAfter.id}. Pronto para reiniciar.` });
        } else {
          setAction({ kind: 'ok', msg: `Pendrive ${stick.name} pronto. Reinicie com o pendrive conectado e selecione-o no menu de boot (F12/F9).` });
          setCd({ mode: 'reboot', secs: 5 });
          return;
        }
        setCd({ mode: 'reboot', secs: 5 });
        return;
      }

      // ---- PASSO 3: nada pronto → erro honesto com o que falta ----
      const faltas: string[] = [];
      if (!isoReady) faltas.push('ISO do Windows 11 não encontrada em ~/Downloads (baixe em microsoft.com)');
      if (!stickReady) faltas.push('Nenhum pendrive ≥ 4 GB conectado');
      if (diskNow && !diskNow.ready && diskNow.blockers.length > 0) {
        faltas.push(...diskNow.blockers);
      }

      setAction({
        kind: 'err',
        error: {
          code: 'SF-BOOT-030',
          message: 'Ainda não há um caminho pronto para instalar o Windows 11.',
          recommendation: faltas.length > 0
            ? 'Faltando: ' + faltas.join(' · ')
            : 'Conecte um pendrive ≥ 4 GB e baixe a ISO do Windows 11 para ~/Downloads.',
        },
      });
      return;
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  /** Gerar autounattend.xml (respostas da instalação). */
  async function generateUnattend(edition: 'pro' | 'home', fullWipe: boolean) {
    setAction({ kind: 'working', msg: 'Gerando autounattend.xml…' });
    try {
      const r = await backend<{ path: string; on_ventoy: boolean }>('unattend_save', {
        edition,
        full_wipe: fullWipe,
      });
      setAction({
        kind: 'ok',
        msg: `autounattend.xml salvo em ${r.path}. Copie para a RAIZ do pendrive antes do boot (ou deixe que o preparo do pendrive otimizado o faça automaticamente).`,
      });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  const usbEntry = efi.state === 'ok' ? efi.data.entries.find(
    (e) => (e.name.toLowerCase().includes('usb') || e.name.toLowerCase().includes('removable')) &&
           (e.device_path ?? '').includes('VenMsg')
  ) : undefined;
  const hasIso = checklist.state === 'ok' && checklist.data.isos.length > 0;
  const ventoyReady = media.some((m) => m.is_ventoy);
  const stickReady = media.some((m) => m.size_bytes > 3_000_000_000);
  const bootNextArmed = efi.state === 'ok' && !!efi.data.boot_next;

  // Determina se o botão principal pode ser clicado (caminho real que completa).
  const canLaunch = (ventoyReady && !!usbEntry) || (stickReady && hasIso && disk?.ready);

  return (
    <div className="page">
      {cd && <CountdownOverlay cd={cd} onCancel={() => { setCd(null); setAction({ kind: 'idle' }); }} />}

      <header className="page-head">
        <h1>Instalar Windows 11</h1>
        <p className="page-sub">
          Fluxo garantido: preparar mídia de boot no pendrive → armar boot automático → reiniciar direto no instalador.
          Sem firulas, sem etapas ocultas.
        </p>
        <div className="status-bar">
          <div className="status-item">
            <span className="dot" style={{ background: hasIso ? '#3ddc84' : '#ff5c6c' }}></span>
            <span>{hasIso ? 'ISO detectada' : 'ISO ausente'} — {hasIso ? checklist.data.isos.map((i) => i.name).join(', ') : 'baixe em ~/Downloads'}</span>
          </div>
          <span className="sep">|</span>
          <div className="status-item">
            <span className="dot" style={{ background: stickReady ? '#3ddc84' : '#ff5c6c' }}></span>
            <span>{stickReady ? 'Pendrive ≥ 4 GB conectado' : 'Nenhum pendrive grande o suficiente'}</span>
          </div>
          <span className="sep">|</span>
          <div className="status-item">
            <span className="dot" style={{ background: bootNextArmed ? '#3ddc84' : '#ffc857' }}></span>
            <span>{bootNextArmed ? 'Boot automático armado' : 'Boot não armado'}</span>
          </div>
        </div>
      </header>

      {checklist.state === 'loading' && <Skeleton />}
      {checklist.state === 'error' && <ErrorCard where="Checklist" error={checklist.error} />}

      {checklist.state === 'ok' && (
        <>
          {/* CARD PRINCIPAL — fluxo único e garantido */}
          <Card title="Instalar agora — passo a passo" tag="fluxo garantido">
            <div className="steps">
              <div className={`step ${hasIso ? 'done' : 'pending'}`}>
                <span className="step-num">1</span>
                <div>
                  <strong>Baixar a ISO do Windows 11</strong>
                  <p className="dim">Salve em ~/Downloads. A ISO oficial tem ~8 GB.</p>
                </div>
              </div>
              <div className={`step ${stickReady ? 'done' : 'pending'}`}>
                <span className="step-num">2</span>
                <div>
                  <strong>Conectar um pendrive ≥ 4 GB</strong>
                  <p className="dim">O SYSFORGE cria a mídia de boot direta nele (apaga o pendrive).</p>
                </div>
              </div>
              <div className={`step ${canLaunch ? 'ready' : 'pending'}`}>
                <span className="step-num">3</span>
                <div>
                  <strong>Reiniciar e instalar</strong>
                  <p className="dim">A máquina boota direto no instalador do Windows 11.</p>
                </div>
              </div>
            </div>

            <div className="btn-row">
              <button
                className="btn-primary"
                disabled={!canLaunch}
                onClick={() => launchInstaller()}
              >
                ⚡ Reiniciar e instalar Windows 11 agora
              </button>
              {!canLaunch && (
                <p className="hint">
                  O botão fica ativo quando a ISO e um pendrive ≥ 4 GB estiverem prontos.
                </p>
              )}
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

          {/* CARD: autounattend (opcional, mas recomendado) */}
          <Card title="Respostas da instalação (autounattend.xml)" tag="opcional">
            <p className="dim">
              Gera o arquivo que responde automaticamente às perguntas do instalador (edição, idioma,
              bypass de TPM/CPU para hardware antigo). Sem ele, o instalador pergunta tudo na tela.
            </p>
            <div className="form-row">
              <label>Edição</label>
              <select defaultValue="pro">
                <option value="pro">Windows 11 Pro</option>
                <option value="home">Windows 11 Home</option>
              </select>
            </div>
            <div className="form-row check">
              <label>
                <input type="checkbox" defaultChecked />
                <span>Modo apagar disco 0 (instalação totalmente automática — <strong className="danger">destrutivo</strong>)</span>
              </label>
            </div>
            <div className="btn-row">
              <button onClick={() => generateUnattend('pro', false)}>Gerar autounattend.xml</button>
            </div>
            <p className="foot-note">
              O unattend inclui bypass LabConfig (TPM/CPU/SecureBoot/RAM) — hardware antigo instala,
              com a ressalva de suporte oficial da Microsoft.
            </p>
          </Card>

          {/* CARD: pendrive otimizado (detalhe do método que realmente funciona) */}
          <Card title="Como a mídia de boot é criada (pendrive de 4 GB)" tag="detalhe técnico">
            <p className="dim">
              A ISO oficial do Windows 11 (~8 GB) não cabe num pendrive de 4 GB. O SYSFORGE exporta
              <b>apenas a edição escolhida</b> comprimida no formato oficial da Microsoft
              (<code>install.esd</code> com LZMS sólido — ~2,5 GB) e cria um pendrive FAT32 padrão:
              a firmware boota nativamente, exatamente como na mídia oficial da Microsoft.
            </p>
            <div className="btn-row">
              <button
                className="btn-outline"
                disabled={!stickReady}
                onClick={() => {
                  // Redireciona a ação: o launchInstaller já cuida disso quando stickReady
                  void launchInstaller();
                }}
              >
                🔨 (Re)preparar pendrive otimizado
              </button>
            </div>
            <p className="foot-note">
              Este cartão é apenas explicativo. O botão principal acima já prepara e reinicia automaticamente.
            </p>
          </Card>

          {/* CARD: método descartado — honestidade sobre por que não funciona */}
          <Card title="Por que não tem 'instalar sem pendrive'" tag="honestidade">
            <p className="dim">
              O método "ISO no disco + GRUB/wimboot" foi <b>removido do fluxo de instalação</b>:
              ele abre o ambiente de recuperação (WinPE) mas o setup do Windows <b>não lê partições
              ext4</b> — então não encontra o install.wim e a instalação não completa. O boot volta
              ao Linux Mint sozinho.
            </p>
            <p className="dim">
              O único caminho que <b>completa a instalação</b> é mídia de boot FAT32 padrão num
              pendrive — é o que o botão principal prepara automaticamente.
            </p>
          </Card>

          {efi.state === 'ok' && <ControlPanel efi={efi.data} showEntrySelector />}
        </>
      )}
    </div>
  );
}