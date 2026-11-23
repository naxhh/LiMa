import * as React from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { apiJson } from "@/lib/api";
import { BAMBU_STATUS_KEY, apiErrorMessage } from "@/lib/bambu";
import type { components } from "@/gen/openapi";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

type BambuLoginResponse = components["schemas"]["BambuLoginResponse"];

type Step = { kind: "password" } | { kind: "code"; account: string } | { kind: "tfa"; tfaKey: string };

/**
 * Bambu account login. The server keeps nothing: it answers with an HttpOnly
 * cookie holding the token, which later imports read.
 */
export function BambuLogin({ onLoggedIn }: { onLoggedIn: () => void }) {
  const qc = useQueryClient();
  const [mode, setMode] = React.useState<"login" | "token">("login");
  const [step, setStep] = React.useState<Step>({ kind: "password" });
  const [account, setAccount] = React.useState("");
  const [password, setPassword] = React.useState("");
  const [code, setCode] = React.useState("");
  const [token, setToken] = React.useState("");

  const done = async () => {
    setPassword("");
    setCode("");
    setToken("");
    await qc.invalidateQueries({ queryKey: BAMBU_STATUS_KEY });
    onLoggedIn();
  };

  const loginM = useMutation({
    mutationFn: async (): Promise<BambuLoginResponse> => {
      if (mode === "token") return apiJson("POST", "/integrations/bambu/token", { token });
      if (step.kind === "code") return apiJson("POST", "/integrations/bambu/login/code", { account: step.account, code });
      if (step.kind === "tfa") return apiJson("POST", "/integrations/bambu/login/tfa", { tfa_key: step.tfaKey, code });
      return apiJson("POST", "/integrations/bambu/login", { account, password });
    },
    onSuccess: async (res) => {
      if (res.status === "logged_in") return done();
      setCode("");
      if (res.status === "code_sent") setStep({ kind: "code", account: account.trim() });
      else if (res.status === "tfa_required" && res.tfa_key) setStep({ kind: "tfa", tfaKey: res.tfa_key });
    },
  });

  // Passwordless: Bambu emails a code. The way in for Google/Apple sign-in accounts.
  const sendCodeM = useMutation({
    mutationFn: (): Promise<BambuLoginResponse> =>
      apiJson("POST", "/integrations/bambu/login/send-code", { account: account.trim() }),
    onSuccess: () => {
      loginM.reset();
      setCode("");
      setStep({ kind: "code", account: account.trim() });
    },
  });

  const canSubmit =
    !loginM.isPending &&
    (mode === "token"
      ? token.trim().length > 0
      : step.kind === "password"
        ? account.trim().length > 0 && password.length > 0
        : code.trim().length > 0);

  return (
    <form
      className="space-y-3 rounded-xl border bg-card p-4"
      onSubmit={(e) => {
        e.preventDefault();
        if (canSubmit) loginM.mutate();
      }}
    >
      <div className="flex items-center justify-between gap-2">
        <div>
          <div className="text-sm font-medium">Log in to Bambu Lab</div>
          <div className="text-xs text-muted-foreground">MakerWorld only lets logged-in accounts download files.</div>
        </div>
        <div className="flex shrink-0 rounded-lg border p-0.5 text-xs">
          {(["login", "token"] as const).map((m) => (
            <button
              key={m}
              type="button"
              onClick={() => {
                setMode(m);
                loginM.reset();
              }}
              className={cn(
                "cursor-pointer rounded-md px-2 py-1",
                mode === m ? "bg-secondary font-medium text-foreground" : "text-muted-foreground",
              )}
            >
              {m === "login" ? "Email" : "Paste token"}
            </button>
          ))}
        </div>
      </div>

      {mode === "token" ? (
        <div className="space-y-1">
          <Textarea
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="Value of the “token” cookie from a logged-in makerworld.com tab"
            className="font-mono text-xs"
          />
          <div className="text-xs text-muted-foreground">
            In the browser dev tools: Application → Cookies → makerworld.com → <code>token</code>.
          </div>
        </div>
      ) : step.kind === "password" ? (
        <div className="grid gap-2 sm:grid-cols-2">
          <Input
            type="email"
            autoComplete="username"
            placeholder="Email"
            value={account}
            onChange={(e) => setAccount(e.target.value)}
          />
          <Input
            type="password"
            autoComplete="current-password"
            placeholder="Password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </div>
      ) : (
        <div className="space-y-1">
          <div className="text-xs text-muted-foreground">
            {step.kind === "code"
              ? `Bambu emailed a code to ${step.account}.`
              : "Enter the code from your authenticator app."}
          </div>
          <Input
            inputMode="numeric"
            autoComplete="one-time-code"
            placeholder="Code"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            autoFocus
          />
        </div>
      )}

      {mode === "login" && step.kind === "password" ? (
        <button
          type="button"
          disabled={!account.trim() || sendCodeM.isPending}
          onClick={() => sendCodeM.mutate()}
          className="cursor-pointer text-xs text-muted-foreground underline-offset-4 hover:text-foreground hover:underline disabled:cursor-not-allowed disabled:opacity-50"
        >
          {sendCodeM.isPending ? "Sending code…" : "Signed up with Google or Apple? Email me a code instead"}
        </button>
      ) : null}

      {loginM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(loginM.error)}</div> : null}
      {sendCodeM.isError ? <div className="text-sm text-destructive">{apiErrorMessage(sendCodeM.error)}</div> : null}

      <div className="flex justify-end gap-2">
        {mode === "login" && step.kind !== "password" ? (
          <Button type="button" variant="ghost" size="sm" onClick={() => setStep({ kind: "password" })}>
            Start over
          </Button>
        ) : null}
        <Button type="submit" size="sm" disabled={!canSubmit}>
          {loginM.isPending ? "Checking…" : step.kind === "password" || mode === "token" ? "Log in" : "Verify code"}
        </Button>
      </div>
      <div className="text-[11px] text-dim">
        Your password goes to Bambu through LiMa and is never stored. The token stays in this browser, and on the server only for MakerWorld collections you keep in sync.
      </div>
    </form>
  );
}
