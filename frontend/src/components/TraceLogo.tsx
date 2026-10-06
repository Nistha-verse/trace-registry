type TraceLogoProps = {
  compact?: boolean
}

export function TraceLogo({ compact = false }: TraceLogoProps) {
  return (
    <div className={`trace-logo ${compact ? 'trace-logo--compact' : ''}`} aria-label="Trace">
      <svg className="trace-logo__mark" viewBox="0 0 80 72" role="img" aria-hidden="true">
        <path className="trace-logo__shell" d="M4 53C11 26 28 8 40 8s29 18 36 45c-9-7-17-12-25-12-5 0-8 2-11 5-3-3-6-5-11-5-8 0-16 5-25 12Z" />
        <path className="trace-logo__canopy" d="M8 48c8-20 21-32 32-32s24 12 32 32c-10-5-19-8-32-8S18 43 8 48Z" />
        <path className="trace-logo__figure" d="M40 21c4 0 7 3 7 7 0 3-2 5-4 6 4 1 8 5 8 11l9 10-5 4-9-9v14H34V50l-9 9-4-4 9-10c0-6 4-10 8-11-3-1-5-3-5-6 0-4 3-7 7-7Z" />
        <path className="trace-logo__handle" d="M58 27v26c0 7-4 11-9 11" />
      </svg>
      <span className="trace-logo__word">TRACE</span>
    </div>
  )
}
