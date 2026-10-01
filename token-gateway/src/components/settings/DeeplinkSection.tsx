/**
 * 設定頁的「Deep Link 匯入」區塊（P4.1 協定註冊狀態）。
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../../lib/api";
import { SectionHead } from "./SectionHead";

export function DeeplinkSection() {
  const qc = useQueryClient();
  const [msg, setMsg] = useState("");
  const proto = useQuery({
    queryKey: ["deeplink_protocol"],
    queryFn: api.deeplinkProtocolState,
  });
  const reg = useMutation({
    mutationFn: api.deeplinkRegister,
    onSuccess: (s) => {
      setMsg(s.registered ? "已註冊" : "註冊後仍未生效，請檢查權限");
      void qc.invalidateQueries({ queryKey: ["deeplink_protocol"] });
    },
    onError: (e) => setMsg(String(e)),
  });
  const unreg = useMutation({
    mutationFn: api.deeplinkUnregister,
    onSuccess: () => {
      setMsg("已取消註冊（之後點連結不會再打開這個 App）");
      void qc.invalidateQueries({ queryKey: ["deeplink_protocol"] });
    },
    onError: (e) => setMsg(String(e)),
  });

  return (
    <div className="glass p-5">
      <SectionHead
        icon="download"
        tile="linear-gradient(160deg, #0A84FF, #0055cc)"
        title="Deep Link 匯入"
        caption="tokengateway:// 一鍵匯入來源／提示詞／MCP／技能"
      />
      <div className="mac-frow">
        <span className="mac-cap">協定註冊</span>
        <div className="min-w-0 flex-1">
          <p className="text-[12px] text-fg/70">
            {proto.isPending
              ? "查詢中…"
              : proto.data?.registered
                ? "已註冊（HKCU\\Software\\Classes\\tokengateway）"
                : "尚未註冊"}
          </p>
          {proto.data?.command && (
            <p className="font-mono text-[10px] break-all text-fg/25">
              {proto.data.command}
            </p>
          )}
        </div>
        <button
          className="btn-ghost shrink-0 px-3 py-1 text-xs disabled:opacity-40"
          disabled={reg.isPending}
          onClick={() => reg.mutate()}
        >
          重新註冊
        </button>
        {proto.data?.registered && (
          <button
            className="btn-ghost shrink-0 px-3 py-1 text-xs text-red-400/80 disabled:opacity-40"
            disabled={unreg.isPending}
            onClick={() => unreg.mutate()}
          >
            取消註冊
          </button>
        )}
      </div>
      <p className="text-[11px] leading-relaxed text-fg/30">
        連結格式（與 cc-switch 的 <span className="font-mono">ccswitch://</span> 相同參數）：
        <span className="font-mono">
          {" "}
          tokengateway://v1/import?resource=provider&app=claude&name=…&endpoint=…
        </span>
        。匯入前一定會先跳出預覽確認；帶有 API Key 的連結只顯示遮罩後的值。
      </p>
      {msg && <p className="pt-1 text-[11px] text-emerald-400/80">{msg}</p>}
    </div>
  );
}
