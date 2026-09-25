import { useEffect, useState } from 'react';
import { backend, toBackendError, fmtBytes, fmtUptime } from '../lib/sysforge';
import type { BackendError, EfiBootState, EspInfo, SystemInfo } from '../types';

type Loadable<T> = { state: 'loading' } | { state: 'error'; error: BackendError } | { state: 'ok'; data: T };

export function useBackend<T>(command: string, args?: Record<string, unknown>): Loadable<T> {
  const [data, setData] = useState<Loadable<T>>({ state: 'loading' });
  useEffect(() => {
    let alive = true;
    backend<T>(command, args)
      .then((d) => alive && setData({ state: 'ok', data: d }))
      .catch((e) => alive && setData({ state: 'error', error: toBackendError(e) }));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [command]);
  return data;
}

export function Card({ title, children, tag }: { title: string; children: React.ReactNode; tag?: string }) {
  return (
    <section className="card">
      <header className="card-head">
        <h2>{title}</h2>
        {tag && <span className="card-tag">{tag}</span>}
      </header>
      <div className="card-body">{children}</div>
    </section>
  );
}

export function KV({ k, v, mono }: { k: string; v: React.ReactNode; mono?: boolean }) {
  return (
    <div className="kv">
      <span className="kv-k">{k}</span>
      <span className={mono ? 'kv-v mono' : 'kv-v'}>{v}</span>
    </div>
  );
}

export function Badge({ kind, children }: { kind: 'ok' | 'warn' | 'err' | 'info'; children: React.ReactNode }) {
  return <span className={`badge ${kind}`}>{children}</span>;
}

export function ErrorCard({ where, error }: { where: string; error: BackendError }) {
  return (
    <Card title={where} tag="erro">
      <div className="error-block">
        <Badge kind="err">{error.code}</Badge>
        <p className="error-msg">{error.message}</p>
        {error.technical && <p className="error-tech">{error.technical}</p>}
        {error.recommendation && <p className="error-rec">→ {error.recommendation}</p>}
      </div>
    </Card>
  );
}

export function Skeleton() {
  return <div className="skeleton" />;
}

export default function Dashboard() {
  const sys = useBackend<SystemInfo>('get_system_info');
  const esp = useBackend<EspInfo>('get_esp');
  const efi = useBackend<EfiBootState>('get_efi_state');

  const bootEntry = efi.state === 'ok' && efi.data.boot_current
    ? efi.data.entries.find((e) => e.id === efi.data.boot_current)
    : null;

  return (
    <div className="page">
      <header className="page-head">
        <h1>Dashboard</h1>
        <p className="page-sub">Estado real do sistema — sondagem in-process, sem daemon, sem cache</p>
      </header>

      {sys.state === 'loading' && <Skeleton />}
      {sys.state === 'error' && <ErrorCard where="Sistema" error={sys.error} />}
      {sys.state === 'ok' && (
        <div className="grid">
          <Card title="Sistema" tag="live">
            <KV k="hostname" v={sys.data.hostname} mono />
            <KV k="SO" v={sys.data.os.pretty_name ?? '—'} />
            <KV k="Kernel" v={`${sys.data.kernel.release} (${sys.data.kernel.arch})`} mono />
            <KV k="CPUs" v={`${sys.data.cpus} × ${sys.data.cpu_model ?? '—'}`} />
            <KV
              k="Memória"
              v={`${fmtBytes((sys.data.memory.total_kb - sys.data.memory.available_kb) * 1024)} em uso de ${fmtBytes(
                sys.data.memory.total_kb * 1024,
              )}`}
            />
            <div className="bar">
              <div className="bar-label">
                <span>memória em uso</span>
                <span>{Math.round(((sys.data.memory.total_kb - sys.data.memory.available_kb) / sys.data.memory.total_kb) * 100)}%</span>
              </div>
              <div className="bar-track">
                <div
                  className="bar-fill"
                  style={{
                    width: `${Math.min(100, ((sys.data.memory.total_kb - sys.data.memory.available_kb) / sys.data.memory.total_kb) * 100)}%`,
                  }}
                />
              </div>
            </div>
            <KV k="Uptime" v={fmtUptime(sys.data.uptime_secs)} />
          </Card>

          <Card title="Boot" tag="UEFI">
            <KV k="Modo" v={sys.data.is_uefi ? 'UEFI nativo' : 'BIOS legado'} />
            <KV
              k="Secure Boot"
              v={
                sys.data.secure_boot.enabled === true ? (
                  <Badge kind="warn">habilitado</Badge>
                ) : sys.data.secure_boot.enabled === false ? (
                  <Badge kind="ok">desabilitado</Badge>
                ) : (
                  <Badge kind="warn">ilegível</Badge>
                )
              }
            />
            {efi.state === 'ok' && (
              <KV
                k="BootNext"
                v={efi.data.boot_next ? <Badge kind="info">{efi.data.boot_next} (one-shot)</Badge> : <span className="dim">não armado</span>}
              />
            )}
            {esp.state === 'ok' && esp.data.mounted ? (
              <>
                <KV
                  k="ESP"
                  v={`${esp.data.device} · ${fmtBytes(esp.data.free_bytes)} livres de ${fmtBytes(esp.data.total_bytes)}`}
                  mono
                />
                <div className="bar">
                  <div className="bar-label">
                    <span>partição ESP</span>
                    <span>{Math.round((esp.data.free_bytes / esp.data.total_bytes) * 100)}% livre</span>
                  </div>
                  <div className="bar-track">
                    <div
                      className={`bar-fill${esp.data.free_bytes / esp.data.total_bytes < 0.15 ? ' warn' : ''}`}
                      style={{ width: `${Math.min(100, (esp.data.free_bytes / esp.data.total_bytes) * 100)}%` }}
                    />
                  </div>
                </div>
              </>
            ) : (
              esp.state === 'ok' && <KV k="ESP" v={<Badge kind="err">não montada (SF-BOOT-002)</Badge>} />
            )}
            {efi.state === 'ok' && bootEntry && (
              <KV k="Boot atual" v={`${bootEntry.id} · ${bootEntry.name}`} mono />
            )}
            {efi.state === 'ok' && (
              <div className="chips">
                {efi.data.boot_order.slice(0, 8).map((id) => {
                  const e = efi.data.entries.find((x) => x.id === id);
                  return (
                    <span key={id} className={`chip${id === efi.data.boot_current ? ' current' : ''}`}>
                      {id} {e ? e.name.slice(0, 14) : ''}
                    </span>
                  );
                })}
              </div>
            )}
          </Card>

          <Card title="Energia & Hardware" tag="live">
            {sys.data.power.batteries.map((b) => (
              <KV
                key={b.name}
                k={b.name}
                v={`${b.capacity_pct ?? '—'}% · ${(b.status ?? '').toLowerCase()}`}
              />
            ))}
            {sys.data.power.ac_online !== null && (
              <KV k="Fonte AC" v={sys.data.power.ac_online ? 'conectada' : 'desconectada'} />
            )}
            <KV k="TPM" v={sys.data.tpm_present ? 'detectado' : 'não detectado'} />
          </Card>
        </div>
      )}
    </div>
  );
}
