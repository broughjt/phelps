import { Route, Switch } from 'wouter'
import NotePage from './components/NotePage'
import './App.css'

function App() {
  return (
    <div className="min-h-screen bg-white">
      <Switch>
        <Route path="/note/:id?" component={NotePage} />
        <Route path="/">
          <div className="flex items-center justify-center min-h-screen">
            <div className="text-center">
              <h1 className="text-2xl font-bold text-gray-900 mb-4">
                Personal Knowledge Base
              </h1>
              <p className="text-gray-600 mb-6">
                Navigate to <code className="bg-gray-100 px-2 py-1 rounded">/note</code> to view a sample note
              </p>
              <a 
                href="/note" 
                className="inline-block bg-blue-600 text-white px-4 py-2 rounded hover:bg-blue-700 transition-colors"
              >
                View Sample Note
              </a>
            </div>
          </div>
        </Route>
      </Switch>
    </div>
  )
}

export default App
