import { useState } from 'react'
import reactLogo from './assets/react.svg'
import viteLogo from '/vite.svg'
import './App.css'
import { ThemeToggle } from "@/components/theme-toggle";


function App() {
  const [count, setCount] = useState(0)

  return (
    <>
      <div className="p-6">
        <div className="flex justify-end">
          <ThemeToggle />
        </div>

        <h1 className="text-2xl font-semibold mt-6">LIMA</h1>
      </div>
      <h1>Vite + React</h1>
      <div className="card">
        <button onClick={() => setCount((count) => count + 1)}>
          count is {count}
        </button>
        <p>
          Edit <code>src/App.tsx</code> and save to test HMR
        </p>
      </div>
      <p className="read-the-docs">
        Click on the Vite and React logos to learn more
      </p>
    </>
  )
}

export default App
