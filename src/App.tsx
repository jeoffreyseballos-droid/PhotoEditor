import { useState } from "react";
import { desktopAvailable } from "./api";
import { HomeScreen } from "./screens/HomeScreen";
import { JobScreen } from "./screens/JobScreen";
import { NewJobForm } from "./components/NewJobForm";
import { PresetsScreen } from "./screens/PresetsScreen";
import { StandaloneTrainingStudioScreen } from "./screens/StandaloneTrainingStudioScreen";

export function App() {
  const [jobId, setJobId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [mainView, setMainView] = useState<"home" | "training" | "presets">(
    "home",
  );
  const desktop = desktopAvailable();

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark" aria-hidden="true">
            ◈
          </span>
          <div>
            Photo Editor<small>DESKTOP WORKSPACE</small>
          </div>
        </div>
        <nav aria-label="Main navigation">
          <button
            className={
              !jobId && mainView === "home" ? "nav-item active" : "nav-item"
            }
            onClick={() => {
              setJobId(null);
              setCreating(false);
              setMainView("home");
            }}
          >
            <span aria-hidden="true">⌂</span> Home
          </button>
          <button
            className={
              !jobId && mainView === "training" ? "nav-item active" : "nav-item"
            }
            onClick={() => {
              setJobId(null);
              setCreating(false);
              setMainView("training");
            }}
          >
            <span aria-hidden="true">✦</span> Training Studio
          </button>
          <button
            className={
              !jobId && mainView === "presets" ? "nav-item active" : "nav-item"
            }
            onClick={() => {
              setJobId(null);
              setCreating(false);
              setMainView("presets");
            }}
          >
            <span aria-hidden="true">◒</span> Presets
          </button>
          {jobId && (
            <div className="nav-item active" aria-current="page">
              <span aria-hidden="true">▧</span> Job
            </div>
          )}
        </nav>
        <div className="sidebar-note">
          <span className="status-dot" /> Local workspace
          <p>
            Your originals stay untouched.
            <br />
            Your jobs stay on this computer.
          </p>
          <small>PHASE 1 · FOUNDATION</small>
        </div>
      </aside>
      <main>
        {!desktop && (
          <div className="notice" role="status">
            Browser preview — local folders and jobs require the Tauri desktop
            app. No sample jobs or simulated processing are shown.
          </div>
        )}
        {creating ? (
          <NewJobForm
            onCancel={() => setCreating(false)}
            onCreated={(job) => {
              setCreating(false);
              setJobId(job.id);
            }}
          />
        ) : jobId ? (
          <JobScreen key={jobId} jobId={jobId} />
        ) : mainView === "training" ? (
          <StandaloneTrainingStudioScreen
            onClose={() => setMainView("home")}
            onViewPresets={() => setMainView("presets")}
          />
        ) : mainView === "presets" ? (
          <PresetsScreen onClose={() => setMainView("home")} />
        ) : (
          <HomeScreen
            onNew={() => setCreating(true)}
            onOpen={setJobId}
            desktop={desktop}
          />
        )}
      </main>
    </div>
  );
}
