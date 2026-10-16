// The only place the UI talks to Rust. Types come from generated bindings.
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { DbInfo } from './bindings/DbInfo'
import type { ErrorInfo } from './bindings/ErrorInfo'

export const inTauri = isTauri()

export const ipc = {
  dbInfo: () => invoke<DbInfo>('db_info'),
  hasMica: () => invoke<boolean>('has_mica'),
  showMainWindow: () => invoke<void>('show_main_window'),
}

export function isErrorInfo(value: unknown): value is ErrorInfo {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value
}
