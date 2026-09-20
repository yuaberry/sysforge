import { useBackend, Card, ErrorCard, Skeleton, KV, Badge } from './Dashboard';
import type { CapabilityReport } from '../types';

const GROUP_LABELS: Record<string, string> = {
  core: 'Essenciais',
  auth: 'Autorização',
  health: 'Saúde de hardware',
  windows: 'Windows',
  virtual: 'Virtualização (testes)',
};

export default function DoctorPage() {
  const caps = useBackend<CapabilityReport>('get_capabilities');

  return (
    <div className="page">
      <header className="page-head">
        <h1>Diagnóstico</h1>
        <p className="page-sub">O que está pronto e o que falta — com o comando exato para instalar</p>
      </header>

      {caps.state === 'loading' && <Skeleton />}
      {caps.state === 'error' && <ErrorCard where="Diagnóstico" error={caps.error} />}

      {caps.state === 'ok' && (
        <div className="grid">
          <Card title="Resumo por grupo" tag="capabilities">
            {Object.entries(caps.data.groups).map(([g, [found, total]]) => (
              <KV
                key={g}
                k={GROUP_LABELS[g] ?? g}
                v={
                  found === total ? (
                    <Badge kind="ok">{`${found}/${total}`}</Badge>
                  ) : (
                    <Badge kind={g === 'core' ? 'err' : 'warn'}>{`${found}/${total}`}</Badge>
                  )
                }
              />
            ))}
            <KV
              k="Headers Tauri (webkit2gtk-4.1)"
              v={
                caps.data.tauri_build_ready ? (
                  <Badge kind="ok">presentes</Badge>
                ) : (
                  <Badge kind="warn">ausentes — app não compila sem eles</Badge>
                )
              }
            />
            <KV
              k="KVM"
              v={caps.data.kvm === 'available' ? <Badge kind="ok">disponível</Badge> : <Badge kind="warn">ausente</Badge>}
            />
            <KV
              k="OVMF (UEFI p/ VM)"
              v={caps.data.ovmf === 'available' ? <Badge kind="ok">presente</Badge> : <Badge kind="warn">pacote ovmf</Badge>}
            />
          </Card>

          <Card title="Ferramentas ausentes" tag="apt">
            {caps.data.tools.filter((t) => !t.found).length === 0 ? (
              <p className="dim">Nenhuma — ambiente completo.</p>
            ) : (
              <>
                <table className="table">
                  <thead>
                    <tr>
                      <th>Ferramenta</th>
                      <th>Grupo</th>
                      <th>Serve para</th>
                      <th>Pacote apt</th>
                    </tr>
                  </thead>
                  <tbody>
                    {caps.data.tools
                      .filter((t) => !t.found)
                      .map((t) => (
                        <tr key={t.name}>
                          <td className="mono">{t.name}</td>
                          <td>{GROUP_LABELS[t.group] ?? t.group}</td>
                          <td className="dim">{t.purpose}</td>
                          <td className="mono">{t.apt_package}</td>
                        </tr>
                      ))}
                  </tbody>
                </table>
                <p className="foot-note">
                  Instalação de uma vez: <code>bash scripts/bootstrap-linux.sh</code>
                </p>
              </>
            )}
          </Card>
        </div>
      )}
    </div>
  );
}
