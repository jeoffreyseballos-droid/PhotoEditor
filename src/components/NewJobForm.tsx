import { useState, type FormEvent } from "react";
import { api, errorMessage } from "../api";
import type { Job } from "../types";
import { FormatHint } from "./FormatHint";

export function NewJobForm({
  onCancel,
  onCreated,
}: {
  onCancel: () => void;
  onCreated: (job: Job) => void;
}) {
  const [name, setName] = useState("");
  const [inputPath, setInputPath] = useState("");
  const [outputPath, setOutputPath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [choosing, setChoosing] = useState(false);

  async function choose(kind: "input" | "output") {
    setError(null);
    setChoosing(true);
    try {
      const path = await api.chooseFolder(`Select ${kind} folder`);
      if (path) (kind === "input" ? setInputPath : setOutputPath)(path);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setChoosing(false);
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!name.trim() || !inputPath || !outputPath) {
      setError("Enter a job name and choose both folders.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      onCreated(
        await api.createJob({
          name: name.trim(),
          input_path: inputPath,
          output_path: outputPath,
        }),
      );
    } catch (error) {
      setError(errorMessage(error));
      setBusy(false);
    }
  }

  return (
    <section className="screen form-screen">
      <div className="eyebrow">LOCAL WORKSPACE / NEW JOB</div>
      <h1>A place for your next shoot.</h1>
      <p className="subtitle">
        Choose your folders. We’ll find your photos and prepare a local contact
        sheet.
      </p>
      <form className="panel job-form" onSubmit={(event) => void submit(event)}>
        <label htmlFor="job-name">Job name</label>
        <input
          id="job-name"
          autoFocus
          maxLength={120}
          placeholder="e.g. September — Studio portraits"
          value={name}
          onChange={(event) => setName(event.target.value)}
          disabled={busy}
          required
        />
        <label htmlFor="input-path">Input folder</label>
        <div className="folder-field">
          <input
            id="input-path"
            value={inputPath}
            readOnly
            placeholder="Select the folder containing your originals"
          />
          <button
            type="button"
            disabled={busy || choosing}
            onClick={() => void choose("input")}
          >
            Browse input
          </button>
        </div>
        <p className="field-hint">
          Includes subfolders. <FormatHint /> Symbolic links are skipped.
        </p>
        <label htmlFor="output-path">Output folder</label>
        <div className="folder-field">
          <input
            id="output-path"
            value={outputPath}
            readOnly
            placeholder="Select a separate output folder"
          />
          <button
            type="button"
            disabled={busy || choosing}
            onClick={() => void choose("output")}
          >
            Browse output
          </button>
        </div>
        <p className="field-hint">
          Reserved for future exports. Nothing will be written here in Phase 1.
          Output may be inside input; its entire subtree is excluded from scans.
          The folders cannot be identical, and input cannot be inside output.
        </p>
        <div className="notice subtle">
          Originals are read-only. RAW recognition and available embedded
          previews do not apply any RAW adjustments.
        </div>
        {error && (
          <div role="alert" className="error">
            {error}
          </div>
        )}
        <div className="form-actions">
          <button type="button" onClick={onCancel} disabled={busy || choosing}>
            Cancel
          </button>
          <button
            className="primary"
            type="submit"
            disabled={
              busy || choosing || !name.trim() || !inputPath || !outputPath
            }
          >
            {busy ? "Starting job…" : "Create & scan job"}
          </button>
        </div>
      </form>
    </section>
  );
}
