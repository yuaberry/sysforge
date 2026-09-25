// Tipos espelhando a saída serde do sysforge-core (snake_case preservado).

export interface OsInfo {
  id?: string;
  name?: string;
  pretty_name?: string;
  version_id?: string;
  version_codename?: string;
  build_id?: string;
  home_url?: string;
}

export interface KernelInfo {
  release: string;
  version: string;
  arch: string;
}

export interface MemoryInfo {
  total_kb: number;
  available_kb: number;
}

export interface Battery {
  name: string;
  capacity_pct?: number | null;
  status?: string | null;
}

export interface SecureBootInfo {
  efi_supported: boolean;
  value_readable: boolean;
  enabled?: boolean | null;
}

export interface SystemInfo {
  hostname: string;
  os: OsInfo;
  kernel: KernelInfo;
  memory: MemoryInfo;
  cpus: number;
  cpu_model?: string | null;
  is_uefi: boolean;
  secure_boot: SecureBootInfo;
  tpm_present: boolean;
  uptime_secs: number;
  power: { batteries: Battery[]; ac_online?: boolean | null };
}

export interface LsblkDevice {
  name: string;
  path?: string | null;
  majmin?: string | null;
  type?: string | null;
  fstype?: string | null;
  mountpoints?: (string | null)[] | null;
  mountpoint?: string | null;
  size: number;
  model?: string | null;
  serial?: string | null;
  uuid?: string | null;
  partlabel?: string | null;
  partuuid?: string | null;
  tran?: string | null;
  ro?: boolean | null;
  rm?: boolean | null;
  children: LsblkDevice[];
}

export interface EfiBootEntry {
  id: string;
  active: boolean;
  name: string;
  device_path?: string | null;
  loader_path?: string | null;
}

export interface EfiBootState {
  boot_current?: string | null;
  boot_next?: string | null;
  timeout_secs?: number | null;
  boot_order: string[];
  entries: EfiBootEntry[];
}

/// Espelha EfiBootState::usb_entry_id() (o método não serializa no JSON).
export function usbEntryId(efi?: EfiBootState | null): string | null {
  if (!efi) return null;
  const found = efi.entries.find(
    (e) =>
      (e.name.toLowerCase().includes('usb') || e.name.toLowerCase().includes('removable')) &&
      (e.device_path ?? '').includes('VenMsg'),
  );
  return found ? found.id : null;
}

export interface EspInfo {
  mounted: boolean;
  mount_point?: string | null;
  device?: string | null;
  fs_type?: string | null;
  mount_options?: string | null;
  restricted_permissions: boolean;
  total_bytes: number;
  free_bytes: number;
}

// Externally-tagged serde enum: "available" ou { "unavailable": {...} }
export type Availability = 'available' | { unavailable: { reason_code: string; reason: string } };

export function isAvailable(a: Availability | undefined | null): boolean {
  return a === 'available';
}

export function availabilityReason(a: Availability | undefined | null): string | null {
  if (a && typeof a === 'object' && 'unavailable' in a) return `${a.unavailable.reason_code}: ${a.unavailable.reason}`;
  return null;
}

export interface SmartReport {
  device: string;
  available: Availability;
  passed?: boolean | null;
  temperature_c?: number | null;
  power_on_hours?: number | null;
  reallocated_sectors?: number | null;
}

export interface ToolStatus {
  name: string;
  group: string;
  purpose: string;
  apt_package: string;
  found: boolean;
  path?: string | null;
}

export interface CapabilityReport {
  tools: ToolStatus[];
  groups: Record<string, [number, number]>;
  kvm: Availability;
  ovmf: Availability;
  memtest86: Availability;
  tauri_build_ready: boolean;
}

// Erro estruturado do backend (espelha SysforgeError)
export interface BackendError {
  code: string;
  message: string;
  technical?: string;
  recommendation?: string;
}

// ---- Checklist Windows 11 ----
export type ItemStatus = 'ok' | 'warn' | 'fail' | 'info';

export interface ChecklistItem {
  id: string;
  status: ItemStatus;
  title: string;
  detail: string;
  hint?: string | null;
}

export interface IsoFile {
  path: string;
  name: string;
  size_bytes: number;
  looks_like_windows11: boolean;
}

export interface RemovableMedia {
  name: string;
  path: string;
  size_bytes: number;
  fstype?: string | null;
  mounted_at?: string | null;
  is_ventoy: boolean;
  model?: string | null;
}

export interface WindowsChecklist {
  items: ChecklistItem[];
  isos: IsoFile[];
  media: RemovableMedia[];
  recommendation: string;
}

/** Sondagem do método disco (sem pendrive) — daemon v1.install.disk_readiness */
export interface DiskBootReadiness {
  iso_path: string | null;
  secure_boot_off: boolean;
  grub_present: boolean;
  update_grub_present: boolean;
  ram_available_mb: number;
  ram_needed_mb: number;
  ram_ok: boolean;
  wimboot_present: boolean;
  wimboot_needs_download: boolean;
  ready: boolean;
  blockers: string[];
}

export interface NetDevice { name: string; kind: string; state: string; connection: string }
export interface WifiNetwork { ssid: string; signal: number; security: string; active: boolean }
export interface NetInfo { nmcli_present: boolean; devices: NetDevice[]; wifi: WifiNetwork[] }
