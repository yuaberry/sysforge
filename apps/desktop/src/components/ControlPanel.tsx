import { useState } from 'react';
import { Card, Badge } from '../pages/Dashboard';
import { backend, toBackendError, usbEntryId } from '../lib/yua';
import type { BackendError, EfiBootState } from '../types';

type ActionState =
  | { kind: 'idle' }
  | { kind: 'working'; msg: string }
  | { kind: 'ok'; msg: string }
  | { kind: 'err'; error: BackendError };

async function daemonCall(method: string, params: Record<string, unknown>): Promise<Record<string, unknown>> {
  return backend<Record<string, unknown>>('daemon_call', { method, params });
}

/**
 * Painel de ações privilegiadas: todas passam pelo daemon system.
 * Se ele não estiver ativo, o primeiro botão o sobe via pkexec — o polkit
 * pede a senha AO USUÁRIO na tela; o app nunca lida com senhas.
 */
export default function ControlPanel({
  efi,
  showEntrySelector = false,
}: {
  efi?: EfiBootState;
  showEntrySelector?: boolean;
}) {
  const [action, setAction] = useState<ActionState>({ kind: 'idle' });
  const [selected, setSelected] = useState<string>('');

  async function run(doing: string, done: string, fn: () => Promise<Record<string, unknown>>) {
    setAction({ kind: 'working', msg: doing });
    try {
      const r = await fn();
      const suffix =
        typeof r['snapshot_path'] === 'string'
          ? ` · snapshot: ${r['snapshot_path']}`
          : typeof r['socket'] === 'string'
            ? ` · ${r['socket']}`
            : '';
      setAction({ kind: 'ok', msg: done + suffix });
    } catch (e) {
      setAction({ kind: 'err', error: toBackendError(e) });
    }
  }

  const confirm = (m: string) => window.confirm(m);
  const usbAuto = usbEntryId(efi);

  return (
    <Card title="Controle da máquina (daemon system + polkit)" tag="privilegiado">
      <p className="dim">
        Toda ação aqui exige o daemon em modo sistema. O polkit pede sua senha na tela quando
        necessário — nenhum privilégio silencioso.
      </p>
      {showEntrySelector && efi && (
        <div className="form-row">
          <label>Entrada para BootNext</label>
          <select value={selected} onChange={(e) => setSelected(e.target.value)}>
            <option value="">— auto (USB do firmware) —</option>
            {efi.entries.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.id} · {entry.name}
                {entry.active ? '' : ' (inativa)'}
              </option>
            ))}
          </select>
        </div>
      )}
      <div className="btn-row">
        <button
          onClick={() =>
            run('Aguardando autorização no diálogo…', 'Daemon system ativo', () =>
              backend<Record<string, unknown>>('ensure_system_daemon', {}),
            )
          }
        >
          Solicitar privilégio (polkit)
        </button>
        <button
          onClick={() => {
            const target = selected || (usbAuto ?? '');
            if (!target) {
              setAction({
                kind: 'err',
                error: {
                  code: 'YUA-BOOT-006',
                  message: 'Nenhuma entrada USB no firmware para auto-detectar — selecione uma entrada na lista.',
                },
              });
              return;
            }
            if (confirm(`Armar BootNext para ${target} no próximo boot? (one-shot — BootOrder intacto)`))
              run('Armando BootNext…', 'BootNext armado', () =>
                daemonCall('v1.boot.set_next', { entry_id: target, confirm: true }),
              );
          }}
        >
          Armar BootNext {selected ? `(${selected})` : '(USB auto)'}
        </button>
        <button
          onClick={() => {
            if (confirm('O computador vai REINICIAR AGORA direto na tela do BIOS/UEFI. Prosseguir?'))
              run('Reiniciando…', 'Reboot → tela do BIOS', () =>
                daemonCall('v1.boot.reboot_to_firmware', { confirm: true }),
              );
          }}
        >
          Reiniciar direto na BIOS
        </button>
        <button
          onClick={() => {
            if (confirm('Reiniciar a máquina AGORA?'))
              run('Reiniciando…', 'Reiniciando', () => daemonCall('v1.system.reboot', { confirm: true }));
          }}
        >
          Reiniciar
        </button>
        <button
          className="danger"
          onClick={() => {
            if (confirm('DESLIGAR a máquina agora?'))
              run('Desligando…', 'Desligando', () => daemonCall('v1.system.poweroff', { confirm: true }));
          }}
        >
          Desligar
        </button>
      </div>
      {efi?.boot_next && (
        <p className="foot-note">
          BootNext armado atualmente: <Badge kind="info">{efi.boot_next}</Badge> (one-shot)
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
  );
}
