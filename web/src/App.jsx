import Dashboard from './pages/Dashboard'
import ThemeToggle from './components/ThemeToggle'

export default function App() {
  return (
    <div className="relative min-h-[100dvh] bg-slate-50">
      <Dashboard />
      <ThemeToggle />
    </div>
  )
}
