import { invoke } from '@tauri-apps/api/core';
import type { BackendError } from '../types';

/** Levantado quando o frontend roda fora do app Tauri (ex.: vite dev puro). */
export class BridgeUnavailable extends Error {
  readonly code = 'YUA-BRIDGE-001';
  constructor() {
    super(
      'Backend Tauri não disponível neste contexto. Abra o app desktop (yua-desktop) ' +
        'para dados reais do sistema — nada de dados falsos aqui.',
    );
    this.name = 'BridgeUnavailable';
  }
}

/** Converte qualquer erro de invoke em BackendError estruturado. */
export function toBackendError(e: unknown): BackendError {
  if (e instanceof BridgeUnavailable) {
    return { code: e.code, message: e.message };
  }
  if (typeof e === 'string') {
    try {
      const parsed = JSON.parse(e) as BackendError;
      if (parsed.code) return parsed;
    } catch {
      /* string crua */
    }
    return { code: 'YUA-IO-99', message: e };
  }
  if (e && typeof e === 'object' && 'code' in (e as Record<string, unknown>)) {
    return e as BackendError;
  }
  return { code: 'YUA-IO-99', message: String(e) };
}

/** Invoca um comando read-only do backend (yua-core in-process). */
export async function backend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!(window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__) {
    throw new BridgeUnavailable();
  }
  return invoke<T>(command, args);
}

export function fmtBytes(b: number | undefined | null): string {
  if (!b || b <= 0) return '0 B';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let v = b;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1).replace('.', ',')} ${units[i]}`;
}

export function fmtUptime(secs: number): string {
  const s = Math.floor(secs);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h >= 24) return `${Math.floor(h / 24)}d ${h % 24}h`;
  if (h > 0) return `${h}h ${m}min`;
  return `${m}min`;
}
