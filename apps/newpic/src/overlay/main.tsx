import React from 'react';
import ReactDOM from 'react-dom/client';
import { Overlay } from './Overlay';

const rootElement = document.getElementById('root');
if (rootElement) {
  ReactDOM.createRoot(rootElement).render(
    <React.StrictMode>
      <Overlay />
    </React.StrictMode>
  );
}
