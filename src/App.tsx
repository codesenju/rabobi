"use client";

import { useEffect, useState } from "react";
import { Home, type AppMode } from "@/components/clone/Home";
import { ToastProvider } from "@/components/clone/Toasts";
import { CloneWizard } from "@/wizards/CloneWizard";
import { BackupWizard } from "@/wizards/BackupWizard";
import { RestoreWizard } from "@/wizards/RestoreWizard";
import { isNativeBackend } from "@/lib/clone/backend";

function Router() {
  const [mode, setMode] = useState<AppMode>("home");
  const [native, setNative] = useState<boolean | null>(null);

  useEffect(() => {
    isNativeBackend().then(setNative);
  }, []);

  const goHome = () => setMode("home");

  if (mode === "clone") return <CloneWizard onHome={goHome} />;
  if (mode === "backup") return <BackupWizard onHome={goHome} />;
  if (mode === "restore") return <RestoreWizard onHome={goHome} />;
  return <Home native={native} onPick={setMode} />;
}

export default function App() {
  return (
    <ToastProvider>
      <Router />
    </ToastProvider>
  );
}
