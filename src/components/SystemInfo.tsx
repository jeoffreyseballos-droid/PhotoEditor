import { formatBytes } from "../format";
import type { MachineResources } from "../types";
export function SystemInfo({ resources }: { resources: MachineResources }) {
  return (
    <details className="system-info">
      <summary>
        System · {resources.logical_cpu_count} logical CPUs ·{" "}
        {formatBytes(resources.total_ram_bytes)} RAM ·{" "}
        {resources.gpu_name ?? "GPU unavailable"}
      </summary>
      <div className="system-details">
        <span>
          {resources.os} · {resources.architecture}
        </span>
        <span>
          RAM: {formatBytes(resources.available_ram_bytes)} available /{" "}
          {formatBytes(resources.total_ram_bytes)} total
        </span>
        {resources.gpus.length ? (
          resources.gpus.map((gpu, index) => (
            <div key={index}>
              <strong>{gpu.model}</strong>
              <span>
                {gpu.vendor ?? "Vendor unknown"} · {gpu.device_type} ·{" "}
                {gpu.memory_model === "unified"
                  ? "Unified memory (shared with system RAM)"
                  : gpu.memory_model === "shared"
                    ? "Shared system memory"
                    : gpu.memory_model === "dedicated"
                      ? "Dedicated graphics memory"
                      : "Memory model unknown"}
              </span>
              <span>
                {gpu.dedicated_vram_bytes !== null
                  ? `${formatBytes(gpu.dedicated_vram_bytes)} dedicated VRAM`
                  : "Dedicated VRAM: not applicable / unavailable"}
              </span>
              <span>{gpu.graphics_api ?? "Graphics API unavailable"}</span>
              <span>
                {gpu.compute_capability ?? "Compute capability not verified"}
              </span>
              <small>{gpu.detection_source}</small>
            </div>
          ))
        ) : (
          <span>{resources.gpu_detection}</span>
        )}
      </div>
    </details>
  );
}
