import { useEffect, useState } from 'react'
import type { UpdateState } from '../../preload/api'

const api = window.instaOptics

/** Status bar item for application updates; hidden unless there is something to say. */
export function UpdateStatus() {
  const [state, setState] = useState<UpdateState>({ status: 'idle' })
  useEffect(() => {
    void api.update.get().then(setState)
    return api.update.onState(setState)
  }, [])
  // "Up to date" and errors from a manual check fade after a few seconds.
  useEffect(() => {
    if (state.status !== 'none' && state.status !== 'error') return
    const timer = setTimeout(() => setState({ status: 'idle' }), 6000)
    return () => clearTimeout(timer)
  }, [state])

  switch (state.status) {
    case 'idle': return null
    case 'checking': return <span className="status-item"><i className="codicon codicon-sync codicon-modifier-spin" /> Checking for updates…</span>
    case 'none': return <span className="status-item"><i className="codicon codicon-check" /> instaOptics is up to date</span>
    case 'error': return <span className="status-item" title={state.message}><i className="codicon codicon-warning" /> Update check failed</span>
    case 'available': return (
      <button className="status-item status-button highlight" onClick={() => api.update.download()} title={state.canInstall ? 'Download and install' : 'Open the download page'}>
        <i className="codicon codicon-cloud-download" /> Update {state.version} available
      </button>
    )
    case 'downloading': return <span className="status-item"><i className="codicon codicon-cloud-download" /> Downloading {state.version} · {state.percent.toFixed(0)}%</span>
    case 'ready': return (
      <button className="status-item status-button highlight" onClick={() => api.update.install()} title="Restart instaOptics to finish updating">
        <i className="codicon codicon-debug-restart" /> Restart to update to {state.version}
      </button>
    )
  }
}
