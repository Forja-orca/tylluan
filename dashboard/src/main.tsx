import { createRoot } from 'react-dom/client'
import './index.css'
import App from './App'
import { NexusProvider } from './hooks/useNexus'

// StrictMode removed: its dev-only double-mount/double-effect cycle doesn't
// play well with imperative WebGL wrapper libraries like react-force-graph-3d
// (KnowledgeCortex3D) -- the underlying kapsule component doesn't reliably
// tear down its <canvas>/three.js renderer on the simulated unmount, so the
// graph tab could end up with two overlapping engines fighting over the
// same physics state. Production builds never had this (StrictMode only
// runs in dev), but it was making dev-server iteration on the graph
// physics unreliable to reason about.
createRoot(document.getElementById('root')!).render(
  <NexusProvider>
    <App />
  </NexusProvider>,
)
