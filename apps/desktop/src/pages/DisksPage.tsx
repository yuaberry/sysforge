import { useBackend, Card, ErrorCard, Skeleton, KV, Badge } from './Dashboard';
import { fmtBytes } from '../lib/yua';
import type { LsblkDevice, SmartReport } from '../types';
import { useEffect, useState } from 'react';
import { backend, toBackendError } from '../lib/yua';
import type { BackendError } from '../types';

function SmartLine({ disk }: { disk: string }) {
  const [smart, setSmart] = useState<SmartReport | null>(null);
  const [err, setErr] = useState<BackendError | null>(null);
  useEffect(() => {
    backend<SmartReport>('get_smart', { disk })
      .then(setSmart)
      .catch((e) => setErr(toBackendError(e)));
  }, [disk]);

  if (err) return <KV k="SMART" v={<Badge kind="err">{err.code}</Badge>} />;
  if (!smart) return <KV k="SMART" v="…" />;
  if (smart.available !== 'available') {
    const reason =
      typeof smart.available === 'object' && 'unavailable' in smart.available
        ? `${smart.available.unavailable.reason_code}: ${smart.available.unavailable.reason}`
        : 'indisponível';
    return <KV k="SMART" v={<span className="dim">{reason}</span>} />;
  }
  return (
    <KV
      k="SMART"
      v={
        smart.passed === true ? (
          <Badge kind="ok">aprovado</Badge>
        ) : smart.passed === false ? (
          <Badge kind="err">REPROVADO</Badge>
        ) : (
          <Badge kind="warn">sem veredito</Badge>
        )
      }
    />
  );
}

function DeviceRow({ dev }: { dev: LsblkDevice }) {
  const mounts = (dev.mountpoints ?? []).filter((m): m is string => m !== null);
  return (
    <tr>
      <td className="mono">{dev.name}</td>
      <td>{dev.partlabel ?? '—'}</td>
      <td>{dev.fstype ?? '—'}</td>
      <td className="mono">{fmtBytes(dev.size)}</td>
      <td className="mono">{mounts.length ? mounts.join(', ') : '—'}</td>
    </tr>
  );
}

export default function DisksPage() {
  const disks = useBackend<LsblkDevice[]>('get_disks');

  return (
    <div className="page">
      <header className="page-head">
        <h1>Discos</h1>
        <p className="page-sub">Inventário lsblk + udev — dados reais, identidade de série visível</p>
      </header>

      {disks.state === 'loading' && <Skeleton />}
      {disks.state === 'error' && <ErrorCard where="Discos" error={disks.error} />}
      {disks.state === 'ok' &&
        disks.data
          .filter((d) => d.type === 'disk')
          .map((d) => (
            <Card
              key={d.name}
              title={`/${d.name}`}
              tag={d.rm ? 'removível' : 'disco fixo'}
            >
              <KV k="modelo" v={d.model ?? '—'} />
              <KV k="serial" v={<span className="mono">{d.serial ?? '—'}</span>} />
              <KV k="tamanho" v={fmtBytes(d.size)} />
              <KV k="transporte" v={d.tran ?? '—'} />
              <SmartLine disk={d.path ?? `/dev/${d.name}`} />
              {d.children.length > 0 && (
                <table className="table">
                  <thead>
                    <tr>
                      <th>Partição</th>
                      <th>Rótulo</th>
                      <th>FS</th>
                      <th>Tamanho</th>
                      <th>Montagem</th>
                    </tr>
                  </thead>
                  <tbody>
                    {d.children.map((p) => (
                      <DeviceRow key={p.name} dev={p} />
                    ))}
                  </tbody>
                </table>
              )}
            </Card>
          ))}
      {disks.state === 'ok' && (
        <p className="foot-note">
          Guard de segurança ativo: operações destrutivas no disco do sistema vivo são recusadas por
          código (YUA-DISK-010). Testes destrutivos reais ocorrerão apenas em VMs (Fase 9).
        </p>
      )}
    </div>
  );
}
