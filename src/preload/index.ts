import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type { SttApi } from '../shared/types'

const api: SttApi = {
  call: (method: string, params?: unknown) => ipcRenderer.invoke('core:call', method, params),
  on: (event: string, listener: (payload: never) => void) => {
    const handler = (_e: IpcRendererEvent, name: string, payload: unknown): void => {
      if (name === event) listener(payload as never)
    }
    ipcRenderer.on('core:event', handler)
    return () => ipcRenderer.removeListener('core:event', handler)
  },
  info: () => ipcRenderer.invoke('app:info'),
  openExternal: (url: string) => ipcRenderer.send('app:open-external', url)
} as SttApi

contextBridge.exposeInMainWorld('stt', api)
