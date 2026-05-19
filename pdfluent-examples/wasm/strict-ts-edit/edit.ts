/**
 * strict-ts-edit — PDFluent WASM strict-TypeScript example.
 *
 * Demonstrates:
 * 1. Typed `PdfDoc.open()` + `getTextPositions()` usage
 * 2. Worker message-passing pattern with fully typed messages
 * 3. `XfaWasmError` structured error handling
 * 4. `using` keyword for automatic WASM memory management
 *
 * Compile check: tsc --strict --noEmit (must exit 0)
 */

import init, { PdfDoc } from '@pdfluent/sdk-wasm';
import type { XfaWasmError } from '@pdfluent/sdk-wasm/augment';

/* ─── Typed text run (matches WASM JSON output schema) ──────────────────── */

interface TextRun {
  text: string;
  x: number;
  y: number;
  width: number;
  height: number;
  fontSize: number;
}

/* ─── Typed worker messages ──────────────────────────────────────────────── */

interface WorkerRequest {
  type: 'extract';
  pdfBytes: Uint8Array;
  pageIndex: number;
}

interface WorkerResponse {
  type: 'result';
  runs: TextRun[];
  pageCount: number;
}

interface WorkerError {
  type: 'error';
  code: string;
  message: string;
}

type WorkerMessage = WorkerResponse | WorkerError;

/* ─── Main-thread helper ─────────────────────────────────────────────────── */

/**
 * Extract text positions from a PDF page using a dedicated Worker.
 *
 * The Worker owns the WASM instance so the main thread is never blocked by
 * WASM initialisation or PDF parsing.
 */
async function extractTextViaWorker(
  pdfBytes: Uint8Array,
  pageIndex: number,
): Promise<WorkerResponse> {
  return new Promise<WorkerResponse>((resolve, reject) => {
    const worker = new Worker(new URL('./edit.worker.js', import.meta.url), { type: 'module' });

    worker.onmessage = (event: MessageEvent<WorkerMessage>) => {
      worker.terminate();
      const msg = event.data;
      if (msg.type === 'result') {
        resolve(msg);
      } else {
        reject(new Error(`[${msg.code}] ${msg.message}`));
      }
    };

    worker.onerror = (err: ErrorEvent) => {
      worker.terminate();
      reject(new Error(err.message));
    };

    const request: WorkerRequest = { type: 'extract', pdfBytes, pageIndex };
    worker.postMessage(request, [pdfBytes.buffer]);
  });
}

/* ─── Worker implementation (inline for single-file compile check) ─────── */

/**
 * Worker entry point. In production, this lives in `edit.worker.ts` which is
 * bundled separately. Here it is included inline so a single `tsc --noEmit`
 * pass can validate the complete type surface.
 */
async function runWorker(request: WorkerRequest): Promise<WorkerMessage> {
  await init();

  try {
    using doc = PdfDoc.open(request.pdfBytes);

    const pageCount: number = doc.pageCount();
    const rawJson: string = doc.getTextPositions(request.pageIndex);
    const runs = JSON.parse(rawJson) as TextRun[];

    const response: WorkerResponse = { type: 'result', runs, pageCount };
    return response;
  } catch (e: unknown) {
    if (e instanceof Error && 'code' in e) {
      const err = e as XfaWasmError;
      const errResponse: WorkerError = {
        type: 'error',
        code: err.code,
        message: err.message,
      };
      return errResponse;
    }
    const errResponse: WorkerError = {
      type: 'error',
      code: 'UNKNOWN',
      message: e instanceof Error ? e.message : String(e),
    };
    return errResponse;
  }
}

/* ─── Example: render page and overlay text runs ────────────────────────── */

/**
 * Full usage example: open a PDF, render page 0, and overlay the text-run
 * bounding boxes on a canvas.
 *
 * ```html
 * <canvas id="viewer"></canvas>
 * <input type="file" id="picker" accept=".pdf" />
 * ```
 */
async function renderWithTextOverlay(canvas: HTMLCanvasElement, pdfBytes: Uint8Array): Promise<void> {
  await init();

  using doc = PdfDoc.open(pdfBytes);

  const pageIndex = 0;
  const scale = 1.5;

  // Render page to canvas (raster path — wasm32 only in browser)
  doc.renderPageToCanvas(canvas, pageIndex, scale);

  // Overlay text-run bounding boxes
  const ctx = canvas.getContext('2d');
  if (ctx === null) return;

  const rawJson: string = doc.getTextPositions(pageIndex);
  const runs = JSON.parse(rawJson) as TextRun[];

  ctx.strokeStyle = 'rgba(0, 120, 255, 0.4)';
  ctx.lineWidth = 1;

  for (const run of runs) {
    ctx.strokeRect(run.x * scale, run.y * scale, run.width * scale, run.height * scale);
  }
}

/* ─── Typed error handling example ──────────────────────────────────────── */

async function openWithErrorHandling(bytes: Uint8Array): Promise<number> {
  await init();

  try {
    using doc = PdfDoc.open(bytes);
    return doc.pageCount();
  } catch (e: unknown) {
    if (e instanceof Error && 'code' in e) {
      const err = e as XfaWasmError;
      switch (err.code) {
        case 'INVALID_PDF':
          throw new Error('Not a valid PDF file');
        case 'PAGE_OUT_OF_RANGE':
          throw new Error('Requested page does not exist');
        default:
          // Re-throw with structured info preserved
          throw new Error(`PDF operation failed [${err.code}]: ${err.message}`);
      }
    }
    throw e;
  }
}

// Suppress unused-declaration warnings for exported symbols
export {
  extractTextViaWorker,
  renderWithTextOverlay,
  openWithErrorHandling,
  runWorker,
};
export type { TextRun, WorkerRequest, WorkerResponse, WorkerError, WorkerMessage };
