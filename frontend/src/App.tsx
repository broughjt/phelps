import './App.css'
import { Route, Switch } from "wouter";
import NotePage from './NotePage.tsx'

export default function App() {
  return (
    <Switch>
      <Route path="/note/:id?" component={NotePage} />
    </Switch>
  );
}
