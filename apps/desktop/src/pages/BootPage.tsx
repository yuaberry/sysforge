import { useBackend, Card, ErrorCard, Skeleton, KV, Badge } from './Dashboard';
import { fmtBytes } from '../lib/yua';
import type { EspInfo, EfiBootState, SecureBootInfo } from '../types';

export default function BootPage() {
  const efi = useBackend<EfiBootState>('get_efi_state');
  const esp = useBackend<EspInfo>('get_esp');
  const sb = useBackend<SecureBootInfo>('get_secure_boot');

  return (
    <div className="page">
      <header className="page-head">
        <h1>Boot / UEFI</h1>
        <p className="page-sub">Leitura real via efibootmgr + efivars — escrita chega no Milestone 1 (BootNext one-shot)</p>
      </header>

      {efi.state === 'loading' && <Skeleton />}
      {efi.state === 'error' && <ErrorCard where="UEFI" error={efi.error} />}

      {efi.state === 'ok' && (
        <>
          <div className="grid">
            <Card title="Firmware" tag="leitura">
              {sb.state === 'ok' && (
                <KV
                  k="Secure Boot"
                  v={
                    sb.data.enabled === true ? (
                      <Badge kind="warn">habilitado</Badge>
                    ) : sb.data.enabled === false ? (
                      <Badge kind="ok">desabilitado</Badge>
                    ) : (
                      <Badge kind="warn">ilegível</Badge>
                    )
                  }
                />
              )}
              <KV k="Timeout" v={`${efi.data.timeout_secs ?? '—'}s`} />
              <KV k="BootOrder" v={<span className="mono">{efi.data.boot_order.join(' → ') || '—'}</span>} />
            </Card>

            {esp.state === 'ok' && (
              <Card title="ESP" tag={esp.data.mounted ? 'montada' : 'ausente'}>
                {esp.data.mounted ? (
                  <>
                    <KV k="dispositivo" v={<span className="mono">{esp.data.device}</span>} />
                    <KV k="filesystem" v={esp.data.fs_type ?? '—'} />
                    <KV k="espaço livre" v={`${fmtBytes(esp.data.free_bytes)} de ${fmtBytes(esp.data.total_bytes)}`} />
                    <KV
                      k="permissões"
                      v={
                        esp.data.restricted_permissions ? (
                          <Badge kind="warn">restritas (root) — escrita via daemon</Badge>
                        ) : (
                          <Badge kind="ok">abertas</Badge>
                        )
                      }
                    />
                  </>
                ) : (
                  <p className="dim">ESP não montada — YUA-BOOT-002.</p>
                )}
              </Card>
            )}
          </div>

          <Card title={`Entradas (${efi.data.entries.length})`} tag="efibootmgr">
            <table className="table">
              <thead>
                <tr>
                  <th>Boot</th>
                  <th>ID</th>
                  <th>Ativo</th>
                  <th>Nome</th>
                  <th>Loader / device path</th>
                </tr>
              </thead>
              <tbody>
                {efi.data.entries.map((e) => (
                  <tr key={e.id} className={e.id === efi.data.boot_current ? 'row-current' : ''}>
                    <td>{e.id === efi.data.boot_current ? <Badge kind="info">▶ atual</Badge> : ''}</td>
                    <td className="mono">{e.id}</td>
                    <td>{e.active ? <Badge kind="ok">sim</Badge> : <span className="dim">não</span>}</td>
                    <td>{e.name}</td>
                    <td className="mono dim">{e.loader_path ?? e.device_path ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="foot-note">
              Nunca alteramos o BootOrder permanente — o plano usa BootNext (one-shot) com snapshot
              prévio do estado original.
            </p>
          </Card>
        </>
      )}
    </div>
  );
}
