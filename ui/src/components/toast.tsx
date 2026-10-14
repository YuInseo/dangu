'use client';

import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react';

interface Toast {
  id: number;
  message: string;
  tone: 'info' | 'error';
}

const ToastCtx = createContext<(message: string, tone?: Toast['tone']) => void>(() => {});

export function useToast() {
  return useContext(ToastCtx);
}

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const seq = useRef(0);
  const push = useCallback((message: string, tone: Toast['tone'] = 'info') => {
    const id = ++seq.current;
    setToasts((ts) => [...ts.filter((t) => t.message !== message), { id, message, tone }].slice(-3));
    setTimeout(() => setToasts((ts) => ts.filter((t) => t.id !== id)), tone === 'error' ? 6000 : 2600);
  }, []);
  return (
    <ToastCtx.Provider value={push}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className={`toast toast-${t.tone}`}>
            {t.message}
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}
