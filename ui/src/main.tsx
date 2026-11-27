import React from 'react';
import ReactDOM from 'react-dom/client';
import { Provider } from 'react-redux';
import App from './App';
import './index.css';
import { initDesktopApiTransport } from './services/desktopBridge';
import { store } from './store';
import { setDesktopMeta } from './store/setupSlice';

async function bootstrap() {
  const desktopInfo = await initDesktopApiTransport();
  if (desktopInfo) {
    store.dispatch(
      setDesktopMeta({
        isDesktop: true,
        platform: desktopInfo.platform,
        version: desktopInfo.version,
        serverBaseUrl: desktopInfo.server_base_url,
      })
    );
  }

  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <Provider store={store}>
        <App />
      </Provider>
    </React.StrictMode>
  );
}

void bootstrap();
