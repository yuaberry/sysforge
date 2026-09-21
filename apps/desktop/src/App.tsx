import { NavLink, Route, Routes } from 'react-router-dom';
import Dashboard from './pages/Dashboard';
import DisksPage from './pages/DisksPage';
import BootPage from './pages/BootPage';
import DoctorPage from './pages/DoctorPage';
import WindowsPage from './pages/WindowsPage';
import Placeholder from './pages/Placeholder';

interface NavItem {
  to: string;
  label: string;
  phase?: number;
}

const NAV_MAIN: NavItem[] = [
  { to: '/', label: 'Dashboard' },
  { to: '/discos', label: 'Discos' },
  { to: '/boot', label: 'Boot / UEFI' },
  { to: '/windows', label: 'Instalar Windows 11' },
  { to: '/doctor', label: 'Diagnóstico' },
];

const NAV_PLANNED: Array<{ to: string; label: string; phase: number; desc: string }> = [
  { to: '/instalacao', label: 'Modo de Instalação', phase: 2, desc: 'Express · Avançado · Automated · Recovery · Custom' },
  { to: '/imagem', label: 'Imagem ISO', phase: 3, desc: 'Seleção, download (netboot) e validação SHA-256' },
  { to: '/particionamento', label: 'Particionamento', phase: 4, desc: 'Plano GPT/MBR com pré-visualização e rollback' },
  { to: '/pos-instalacao', label: 'Pós-instalação', phase: 6, desc: 'Scripts, aplicativos e configurações' },
  { to: '/drivers', label: 'Drivers', phase: 6, desc: 'SDI/Windows DevOps — offline injection' },
  { to: '/aplicativos', label: 'Aplicativos', phase: 6, desc: 'Pacotes declarativos pós-install' },
  { to: '/contas', label: 'Contas & Chaves', phase: 5, desc: 'Usuários e ativação — nada é logado em texto puro' },
  { to: '/redes', label: 'Redes', phase: 7, desc: 'nmcli: Wi-Fi, VLAN, proxy' },
  { to: '/verificacao', label: 'Verificação', phase: 8, desc: 'Pós-boot: health-check automático' },
];

export default function App() {
  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">YUA</span>
          <span className="brand-sub">OS MANAGER</span>
        </div>
        <nav>
          <div className="nav-label">Operação</div>
          {NAV_MAIN.map((item) => (
            <NavLink key={item.to} to={item.to} end={item.to === '/'} className={({ isActive }) => (isActive ? 'nav-item active' : 'nav-item')}>
              {item.label}
            </NavLink>
          ))}
          <div className="nav-label">Fluxo de instalação</div>
          {NAV_PLANNED.map((item) => (
            <NavLink key={item.to} to={item.to} className={({ isActive }) => (isActive ? 'nav-item active' : 'nav-item')}>
              <span>{item.label}</span>
              <span className="phase-chip">F{item.phase}</span>
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-foot">
          <div className="foot-status ok">núcleo v0.1.0 · protocolo IPC v1</div>
          <div className="foot-note">leitura in-process · escrita via daemon</div>
        </div>
      </aside>
      <main className="main">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/discos" element={<DisksPage />} />
          <Route path="/boot" element={<BootPage />} />
          <Route path="/doctor" element={<DoctorPage />} />
          {NAV_PLANNED.map((item) => (
            <Route key={item.to} path={item.to} element={<Placeholder title={item.label} phase={item.phase} desc={item.desc} />} />
          ))}
          <Route path="/windows" element={<WindowsPage />} />
        </Routes>
      </main>
    </div>
  );
}
