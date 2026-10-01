import { useEffect, useRef, useState } from "react";
import {
  QueryClient,
  QueryClientProvider,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { api, useGatewayPort } from "./lib/api";
import { applyAccentVars, isAccentId, loadAccent, saveAccentLocal } from "./lib/theme";

/** 強調色同步：localStorage 即時首幀，後端 SQLite 為真相源（啟動永不回寫覆蓋）。 */
function AccentSync() {
  const q = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    staleTime: Infinity,
  });
  useEffect(() => {
    const a = q.data?.accent;
    if (a && isAccentId(a)) {
      applyAccentVars(a);
      saveAccentLocal(a);
    }
  }, [q.data]);
  return null;
}
import ProvidersPage from "./components/Providers";
import { I18nProvider, loadLang, useI18n } from "./lib/i18n";
import KeysPage from "./components/Keys";
import SettingsPage from "./components/Settings";
import DiagnosticsPage from "./components/Diagnostics";
import UsagePage, {
  UsageFilterBar,
  type UsageFilterState,
} from "./components/Usage";
import CalcPage from "./components/Calc";
import { Icon, type IconName } from "./components/icons";
import { TraySwitchListener } from "./components/TraySwitchListener";
import { DeeplinkDialog } from "./components/DeeplinkDialog";
import "./index.css";

const queryClient = new QueryClient();

type Tab =
  | "usage"
  | "providers"
  | "keys"
  | "diagnostics"
  | "calc"
  | "settings";

/** 導覽項目：`label` 換成字典 key，實際文字由 `t()` 決定（P4.11 多語系）。 */
const TABS: { id: Tab; label: string; icon: IconName; iconFill: IconName }[] = [
  { id: "usage", label: "nav.usage", icon: "chart-bar", iconFill: "chart-bar-fill" },
  { id: "providers", label: "nav.providers", icon: "server", iconFill: "server" },
  { id: "keys", label: "nav.keys", icon: "key", iconFill: "key-fill" },
  { id: "diagnostics", label: "nav.diagnostics", icon: "alert", iconFill: "bolt-fill" },
  { id: "calc", label: "nav.calc", icon: "calculator", iconFill: "calculator" },
  { id: "settings", label: "nav.settings", icon: "gear", iconFill: "gear-fill" },
];

/** Toolbar 右側網關控制：狀態點 + 端口 + 啟動/停止。 */
function ProxyPill() {
  const { t } = useI18n();
  const qc = useQueryClient();
  const port = useGatewayPort();
  const status = useQuery({
    queryKey: ["proxy_status"],
    queryFn: api.proxyStatus,
    refetchInterval: 5000,
  });
  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
  });
  const autoTried = useRef(false);

  // 開機自啟（每個會話只試一次）
  useEffect(() => {
    if (
      !autoTried.current &&
      settings.data?.auto_start_proxy &&
      status.data &&
      !status.data.running
    ) {
      autoTried.current = true;
      api
        .proxyStart(settings.data.gateway_port)
        .then(() => {
          void qc.invalidateQueries({ queryKey: ["proxy_status"] });
        })
        .catch(() => {});
    }
  }, [settings.data, status.data, qc]);

  const start = useMutation({
    mutationFn: () => api.proxyStart(port),
    onSuccess: () =>
      void qc.invalidateQueries({ queryKey: ["proxy_status"] }),
  });
  const stop = useMutation({
    mutationFn: api.proxyStop,
    onSuccess: () =>
      void qc.invalidateQueries({ queryKey: ["proxy_status"] }),
  });

  const running = status.data?.running ?? false;
  return (
    <div
      className="flex items-center gap-2.5"
      title={start.isError || stop.isError ? String(start.error || stop.error) : undefined}
    >
      <span
        className="inline-block h-2 w-2 rounded-full"
        style={{
          background: running ? "var(--mac-green)" : "rgba(255,255,255,0.18)",
          boxShadow: running ? "0 0 6px rgba(48,209,88,0.7)" : "none",
        }}
      />
      <span className="hidden font-mono text-xs sm:inline" style={{ color: "var(--mac-label-3)" }}>
        127.0.0.1:{status.data?.port || port}
      </span>
      {running ? (
        <button
          className="btn-ghost flex items-center gap-1.5 px-3 py-1 text-xs"
          onClick={() => stop.mutate()}
        >
          <Icon name="stop-fill" size={10} />
          {t("top.stop")}
        </button>
      ) : (
        <button
          className="btn-primary flex items-center gap-1.5 px-3 py-1 text-xs"
          disabled={start.isPending}
          onClick={() => start.mutate()}
        >
          <Icon name="play-fill" size={10} />
          {t("top.start")}
        </button>
      )}
    </div>
  );
}

function Shell() {
  const { t } = useI18n();
  const [tab, setTab] = useState<Tab>("usage");
  const active = TABS.find((item) => item.id === tab)!;
  const [uFilter, setUFilter] = useState<UsageFilterState>({
    range: "today",
    app: "",
    providerId: "",
    model: "",
    custom: null,
  });
  const [showImport, setShowImport] = useState(false);
  const patchFilter = (p: Partial<UsageFilterState>) =>
    setUFilter((f) => ({ ...f, ...p }));
  useEffect(() => {
    applyAccentVars(loadAccent());
  }, []);
  return (
    <div className="relative z-[1] flex h-screen">
      <AccentSync />
      {/* 系統匣切換的接收端：掛在全域，任何頁面都能收到托盤的切換（P1.6） */}
      <TraySwitchListener />
      {/* Deep Link 匯入確認（P4.1）：啟動參數或執行中的 deeplink 事件都會跳出來 */}
      <DeeplinkDialog />
      {/* ── 環境光暈 ── */}
      <div className="bg-orbs" aria-hidden="true">
        <div
          className="orb"
          style={{
            width: 520, height: 520, top: "-160px", right: "-100px",
            background: "rgba(10,132,255,0.30)", animationDuration: "24s",
          }}
        />
        <div
          className="orb"
          style={{
            width: 460, height: 460, bottom: "-180px", left: "10%",
            background: "rgba(124,92,255,0.24)", animationDuration: "29s",
          }}
        />
        <div
          className="orb"
          style={{
            width: 380, height: 380, top: "36%", left: "-150px",
            background: "rgba(48,200,220,0.17)", animationDuration: "19s",
          }}
        />
        <div
          className="orb"
          style={{
            width: 340, height: 340, bottom: "-120px", right: "8%",
            background: "rgba(45,212,191,0.13)", animationDuration: "26s",
          }}
        />
        <div
          className="orb"
          style={{
            width: 300, height: 300, top: "-110px", left: "30%",
            background: "rgba(244,114,182,0.10)", animationDuration: "21s",
          }}
        />
      </div>
      {/* ── Sidebar（<md 收合為圖標軌） ── */}
      <aside className="mac-sidebar m-2 flex w-14 shrink-0 flex-col items-center px-1.5 py-3 md:w-52 md:items-stretch md:px-2.5">
        <div className="flex items-center gap-2 px-2 pb-3">
          <span
            className="flex h-8 w-8 items-center justify-center rounded-[9px] text-fg"
            style={{
              background: "linear-gradient(160deg, #3395ff, #0a64d8)",
              boxShadow: "0 2px 8px rgba(10,132,255,0.35)",
            }}
          >
            <Icon name="chart-bar-fill" size={17} />
          </span>
          <div className="hidden md:block">
            <div className="text-[13px] font-semibold tracking-tight text-fg">
              Token Gateway
            </div>
            <div
              className="text-[10px]"
              style={{ color: "var(--mac-label-4)" }}
            >
              本地 AI 網關
            </div>
          </div>
        </div>
        <nav className="flex w-full flex-col gap-0.5">
          {TABS.map((item) => (
            <button
              key={item.id}
              onClick={() => setTab(item.id)}
              title={t(item.label)}
              data-active={tab === item.id}
              className="mac-side-item flex items-center justify-center gap-2.5 px-2.5 py-[7px] text-[13px] font-medium md:justify-start"
            >
              <Icon
                name={tab === item.id ? item.iconFill : item.icon}
                size={16}
                className="mac-side-icon shrink-0"
              />
              <span className="hidden md:inline">{t(item.label)}</span>
            </button>
          ))}
        </nav>
        <div className="flex-1" />
        <div
          className="hidden px-2.5 pt-3 text-[10px] md:block"
          style={{ color: "var(--mac-label-4)" }}
        >
          v2 · Tauri
        </div>
      </aside>
      {/* ── 主區 ── */}
      <div className="flex min-w-0 flex-1 flex-col py-2 pr-2">
        <div className="mx-auto w-full max-w-6xl shrink-0 px-3 md:px-5">
          <header className="mac-floatbar">
            <div className="flex items-center justify-between gap-3 px-4 py-2.5 md:gap-4 md:px-5">
              <h1 className="min-w-0 truncate text-[15px] font-semibold tracking-tight text-fg">
                {t(active.label)}
              </h1>
              <ProxyPill />
            </div>
            {tab === "usage" && (
              <div className="border-t border-fg/[0.06] px-4 py-2 md:px-5">
                <UsageFilterBar
                  filter={uFilter}
                  onFilter={patchFilter}
                  onImport={() => setShowImport(true)}
                />
              </div>
            )}
          </header>
        </div>
        <main className="scroll-edge mx-auto w-full max-w-6xl flex-1 overflow-y-auto px-3 py-4 md:px-5 md:py-5">
          <div key={tab} className="anim-rise">
            {tab === "usage" && (
              <UsagePage
                filter={uFilter}
                onFilter={patchFilter}
                showImport={showImport}
                onCloseImport={() => setShowImport(false)}
              />
            )}
            {tab === "calc" && <CalcPage />}
            {tab === "providers" && <ProvidersPage />}
            {tab === "keys" && <KeysPage />}
            {tab === "diagnostics" && <DiagnosticsPage />}
            {tab === "settings" && <SettingsPage />}
          </div>
        </main>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      {/* 多語系（P4.11）：首幀用 localStorage 快取，之後由後端 `settings.lang` 覆蓋 */}
      <I18nProvider initial={loadLang()}>
        <Shell />
      </I18nProvider>
    </QueryClientProvider>
  );
}
