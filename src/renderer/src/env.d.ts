/// <reference types="vite/client" />
import type { SttApi } from '../../shared/types'

declare global {
  interface Window {
    stt: SttApi
  }
}
