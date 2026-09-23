import { Card } from './Dashboard';

/** Página de fluxo ainda não implementada — estado HONESTO, sem UI falsa. */
export default function Placeholder({ title, phase, desc }: { title: string; phase: number; desc: string }) {
  return (
    <div className="page">
      <header className="page-head">
        <h1>{title}</h1>
        <p className="page-sub">{desc}</p>
      </header>
      <Card title="Planejado — Fase de implementação futura" tag={`Fase ${phase}`}>
        <p>
          Esta seção ainda não está implementada — e <strong>nenhum botão aqui vai fingir que está</strong>.
          A política do projeto é explicita: operação indisponível é declarada com código e motivo,
          nunca mockada.
        </p>
        <ul className="plain-list">
          <li>Ferramentas e engines correspondentes estão sendo construídas em <code>sysforge-core</code>.</li>
          <li>Operações privilegiadas desta fase passarão pelo daemon <code>sysforge-osd</code> com polkit.</li>
          <li>Acompanhe o progresso real em <code>IMPLEMENTATION_STATUS.md</code>.</li>
        </ul>
      </Card>
    </div>
  );
}
