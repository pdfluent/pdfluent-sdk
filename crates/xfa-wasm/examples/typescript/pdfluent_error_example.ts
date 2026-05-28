/**
 * TypeScript strict-mode example: handling `PdfluentError` from
 * `@pdfluent/sdk-wasm`. Compiles cleanly under `tsc --strict --noEmit`.
 *
 * In a real consumer the import would be:
 *   import { init, PdfDocument, XfaForms, isPdfluentError } from '@pdfluent/sdk-wasm';
 * For the in-tree gate we point at the local TypeScript wrapper.
 */

import {
  init,
  PdfDocument,
  XfaForms,
  isPdfluentError,
  type PdfluentError,
} from '../../ts/index';

async function main(bytes: Uint8Array): Promise<void> {
  await init();

  // ---- Open a (possibly invalid) PDF and discriminate the failure mode ----
  try {
    using doc = PdfDocument.open(bytes);
    console.log(`opened ${doc.pageCount} pages`);
  } catch (e: unknown) {
    if (isPdfluentError(e)) {
      const err: PdfluentError = e;
      const code: string = err.code;
      const operation: string = err.operation;
      const legacy: string = err.legacyCode;

      switch (code) {
        case 'E-PARSE-INVALID-PDF':
          console.error(`[${code}] not a PDF (in ${operation})`);
          break;
        case 'E-WASM-PAGE-OUT-OF-RANGE':
          console.error(`[${code}] page index out of range`);
          break;
        case 'E-LICENSE-INVALID':
          console.error(`[${code}] license invalid`);
          break;
        default:
          console.error(`[${code}] ${err.message} (legacy=${legacy})`);
      }

      if (err.legacyCode === 'INVALID_PDF') {
        console.warn('compat: caller was using legacy code, please migrate');
      }
    } else {
      throw e;
    }
  }

  // ---- XfaForms: typed FormCalc failure ----
  try {
    const forms = XfaForms.fromFields([
      { name: 'Total', value: '', calculate: '10 + 20' },
    ]);
    try {
      forms.runCalculations();
      const total: string | undefined = forms.getFieldValue('form1.Total');
      console.log(`Total = ${total ?? '(unset)'}`);
    } finally {
      forms.free();
    }
  } catch (e: unknown) {
    if (isPdfluentError(e) && e.code === 'E-WASM-FORMCALC-FAILED') {
      console.error(`FormCalc script failed: ${e.message}`);
    } else {
      throw e;
    }
  }
}

export { main };
