import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { useCallback, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui/button";
import { setSetting } from "@/services/settings";
import { WizardStepper } from "@/components/onboarding/WizardStepper";
import {
  WorkspaceStep,
  type WorkspaceStepMode,
} from "@/components/onboarding/WorkspaceStep";
import { DepsSection } from "@/components/onboarding/DepsStep";
import { AsrStep } from "@/components/onboarding/AsrStep";

type WizardStep =
  | "choose"
  | "import"
  | "create"
  | "deps"
  | "asr";

interface WelcomeSearch {
  name?: string;
  path?: string;
  step?: WizardStep;
  /** JSON-encoded adapter_config (e.g. SMB mount info) */
  adapter_config?: string;
  /** 入口来源，例如 "settings" 表示从设置页再次进入 */
  from?: string;
}

const STEP_ITEMS = [
  { key: "workspace", title: "创建工作区", description: "导入或新建" },
  { key: "deps", title: "安装依赖", description: "ffmpeg 等工具" },
  { key: "asr", title: "配置语音识别", description: "可选" },
];

function stepToIndex(step: WizardStep): number {
  if (step === "deps") return 1;
  if (step === "asr") return 2;
  return 0;
}

function WelcomePage() {
  const navigate = useNavigate();
  const {
    name: qName,
    path: qPath,
    step: qStep,
    adapter_config: qAdapterConfig,
    from: qFrom,
  } = Route.useSearch();
  const [hasExisting, setHasExisting] = useState(false);
  const [step, setStep] = useState<WizardStep>(qStep || "choose");
  const [mode, setMode] = useState<WorkspaceStepMode>(
    qStep === "import" || qStep === "create" ? qStep : "choose"
  );

  const handleWorkspaceModeChange = useCallback((m: WorkspaceStepMode) => {
    setMode(m);
    setStep(m === "choose" ? "choose" : m);
  }, []);

  const handleWorkspaceCreated = useCallback(() => {
    setStep("deps");
  }, []);

  const finishOnboarding = useCallback(async () => {
    try {
      await setSetting("onboarding_completed", "true");
    } catch (e) {
      console.warn("Failed to mark onboarding complete:", e);
    }
    navigate({ to: "/dashboard/videos" });
  }, [navigate]);

  const handleSkipWorkspace = () => setStep("deps");

  const handleSkipDeps = async () => {
    try {
      await setSetting("onboarding_deps_skipped", "true");
    } catch (e) {
      console.warn(e);
    }
    setStep("asr");
  };

  const handleSkipAsr = async () => {
    try {
      await setSetting("asr_mode", "disabled");
    } catch (e) {
      console.warn(e);
    }
    finishOnboarding();
  };

  const handleAsrSaved = () => {
    finishOnboarding();
  };

  const handleBackToWorkspace = () => {
    setStep("choose");
    setMode("choose");
  };

  const handleBackToDeps = () => setStep("deps");

  const onWorkspaceStep = step === "choose" || step === "import" || step === "create";
  const stepIndex = onWorkspaceStep ? 0 : stepToIndex(step);

  const fromSettings = qFrom === "settings";

  return (
    <div className="relative flex h-screen justify-center">
      {fromSettings && (
        <Button
          variant="ghost"
          size="sm"
          className="absolute top-4 left-4"
          onClick={() => navigate({ to: "/dashboard/settings" })}
        >
          <ArrowLeft className="mr-1 h-4 w-4" />
          返回设置
        </Button>
      )}
      <div className="mx-auto flex h-full w-full max-w-4xl flex-col p-6">
        <div className="shrink-0 space-y-6">
          <div className="space-y-2 text-center">
            <h1 className="text-3xl font-bold">
              {hasExisting && onWorkspaceStep
                ? "添加工作区"
                : "欢迎使用 ClipperStudio"}
            </h1>
            <p className="text-muted-foreground text-sm">
              面向录播切片创作者的开源视频工作台
            </p>
          </div>

          <WizardStepper steps={STEP_ITEMS} current={stepIndex} />

          {onWorkspaceStep && hasExisting && (
            <div className="flex justify-center">
              <Button
                variant="secondary"
                size="sm"
                onClick={handleSkipWorkspace}
              >
                已有工作区，跳过 →
              </Button>
            </div>
          )}
        </div>

        <div className="mt-6 min-h-0 flex-1 overflow-y-auto pr-1">
          {onWorkspaceStep && (
            <WorkspaceStep
              mode={mode}
              onModeChange={handleWorkspaceModeChange}
              initialName={qName}
              initialPath={qPath}
              adapterConfig={qAdapterConfig}
              onHasExistingChange={setHasExisting}
              onCreated={handleWorkspaceCreated}
            />
          )}

          {step === "deps" && (
            <DepsSection
              intro={
                <p className="text-sm text-muted-foreground">
                  ClipperStudio 需要以下工具。你可以一键下载安装，或点击「自定义路径」指向已安装的可执行文件，也可以暂时跳过。
                </p>
              }
            />
          )}

          {step === "asr" && <AsrStep onSaved={handleAsrSaved} />}
        </div>

        {(step === "deps" || step === "asr") && (
          <div className="shrink-0 border-t bg-background/95 pt-4 backdrop-blur">
            {step === "deps" && (
              <div className="flex items-center justify-between">
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleBackToWorkspace}
                >
                  <ArrowLeft className="mr-1 h-4 w-4" />
                  上一步
                </Button>
                <div className="flex gap-2">
                  <Button variant="ghost" onClick={handleSkipDeps}>
                    稍后再装
                  </Button>
                  <Button onClick={() => setStep("asr")}>下一步</Button>
                </div>
              </div>
            )}

            {step === "asr" && (
              <div className="flex items-center justify-between">
                <Button variant="ghost" size="sm" onClick={handleBackToDeps}>
                  <ArrowLeft className="mr-1 h-4 w-4" />
                  上一步
                </Button>
                <Button variant="ghost" onClick={handleSkipAsr}>
                  稍后再配置
                </Button>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export const Route = createFileRoute("/welcome")({
  component: WelcomePage,
  validateSearch: (search: Record<string, unknown>): WelcomeSearch => ({
    name: (search.name as string) || undefined,
    path: (search.path as string) || undefined,
    step: (["import", "create", "deps", "asr"].includes(search.step as string)
      ? (search.step as WizardStep)
      : undefined),
    adapter_config: (search.adapter_config as string) || undefined,
    from: (search.from as string) || undefined,
  }),
});
