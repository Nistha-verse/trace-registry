import { AppShell } from './components/AppShell'
import { HomePage } from './pages/HomePage'
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
  const programMatch = hash.match(/^#\/program\/(.+)$/)
  const programAddress = programMatch ? decodeURIComponent(programMatch[1]) : null

  return (
    <AppShell>
      {programAddress ? <ProgramPage address={programAddress} /> : <HomePage />}
    </AppShell>
  )
}
