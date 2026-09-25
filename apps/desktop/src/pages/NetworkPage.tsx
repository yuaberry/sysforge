import { useEffect, useState } from 'react';
import { Card, ErrorCard, Skeleton, KV, Badge, useBackend } from './Dashboard';
import { fmtBytes, backend, toBackendError } from '../lib/sysforge';
import type { BackendError, NetInfo } from '../types';

type Sel = { state: 'loading' } | { state: 'error'; error: BackendError } | { state: 'ok'; data: NetInfo };

export default function NetworkPage() {
  const [net, setNet] = useState<Sel>({ state: 'loading' });
  useEffect(() => {
    backend<NetInfo>('get_network_info', {})
      .then((d) => setNet({ state: 'ok', data: d }))
      .catch((e) => setNet({ state: 'error', error: toBackendError(e) }));
  }, []);

  return (
    <div className="page">
      <header className="page-head">
        <h1>Redes</h1>
        <p className="page-sub">Sondagem real via NetworkManager (nmcli) — leitura apenas: nada é configurado aqui.</p>
      </header>
      {net.state === 'loading' && <Skeleton />}
      {net.state === 'error' && <ErrorCard where="Redes" error={net.error} />}
      {net.state === 'ok' && !net.data.nmcli_present && (
        <Card title="NetworkManager" tag="ausente">
          <p className="dim">nmcli não encontrado neste sistema — a página de redes exige NetworkManager (padrão em Mint/Ubuntu).</p>
        </Card>
      )}
      {net.state === 'ok' && net.data.nmcli_present && (
        <div className="grid">
          <Card title="Dispositivos" tag="nmcli device status">
            {net.data.devices.map((d) => (
              <KV
                key={d.name}
                k={d.name}
                v={
                  d.state === 'conectado' || d.state === 'connected'
                    ? <Badge kind="ok">{d.state} · {d.connection || '—'}</Badge>
                    : <span className="badge">{d.state}</span>
                }
              />
            ))}
            {net.data.devices.length === 0 && <p className="dim">Nenhum dispositivo de rede reportado.</p>}
          </Card>
          <Card title="Redes Wi-Fi visíveis" tag="sondagem ao vivo">
            {net.data.wifi.length === 0 && <p className="dim">Nenhum Wi-Fi visível agora (sem rádio ativo ou fora do alcance).</p>}
            {net.data.wifi.slice(0, 10).map((w, i) => (
              <KV
                key={`${w.ssid}-${i}`}
                k={w.active ? '▶ ' + w.ssid : w.ssid}
                v={
                  <>
                    <Badge kind={w.active ? 'ok' : 'info'}>{w.signal}%</Badge>{' '}
                    <span className="dim">{w.security || 'aberta'}</span>
                  </>
                }
              />
            ))}
          </Card>
        </div>
      )}
    </div>
  );
}
