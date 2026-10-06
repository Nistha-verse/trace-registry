import { AppShell } from './components/AppShell'
import { DependenciesPage } from './pages/DependenciesPage'
import { HomePage } from './pages/HomePage'
import { HistoryPage } from './pages/HistoryPage'
import { ProgramPage } from './pages/ProgramPage'
import { useEffect, useState } from 'react'

function useHash() {
  const [hash, setHash] = useState(window.location.hash)
  useEffect(() => {
    const update = () => setHash(window.location.hash)
    window.addEventListener('hashchange', update)
    return () => window.removeEventListener('hashchange', update)
  }, [])
  return hash
}

export default function App() {
  const hash = useHash()
  const detailMatch = hash.match(/^#\/program\/([^/]+)\/(dependencies|history)$/)
  const programMatch = hash.match(/^#\/program\/(.+)$/)
  const programAddress = detailMatch ? decodeURIComponent(detailMatch[1]) : programMatch ? decodeURIComponent(programMatch[1]) : null

  return (
    <AppShell>
      {programAddress && detailMatch?.[2] === 'dependencies' ? <DependenciesPage address={programAddress} /> : null}
      {programAddress && detailMatch?.[2] === 'history' ? <HistoryPage address={programAddress} /> : null}
      {programAddress && !detailMatch ? <ProgramPage address={programAddress} /> : null}
      {!programAddress ? <HomePage /> : null}
    </AppShell>
  )
}
