package com.xfa.pdf

import android.graphics.Bitmap
import java.nio.ByteBuffer

/**
 * An RGBA-rendered page image.
 *
 * @property width Image width in pixels.
 * @property height Image height in pixels.
 * @property pixels Raw RGBA pixel data (4 bytes per pixel).
 */
class RenderedImage internal constructor(
    val width: Int,
    val height: Int,
    val pixels: ByteArray
) {
    /**
     * Convert to an Android [Bitmap].
     *
     * The returned Bitmap uses [Bitmap.Config.ARGB_8888] format.
     */
    fun toBitmap(): Bitmap {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        // RGBA -> ARGB conversion: Android expects ARGB_8888
        val argbPixels = IntArray(width * height)
        for (i in argbPixels.indices) {
            val offset = i * 4
            val r = pixels[offset].toInt() and 0xFF
            val g = pixels[offset + 1].toInt() and 0xFF
            val b = pixels[offset + 2].toInt() and 0xFF
            val a = pixels[offset + 3].toInt() and 0xFF
            argbPixels[i] = (a shl 24) or (r shl 16) or (g shl 8) or b
        }
        bitmap.setPixels(argbPixels, 0, width, 0, 0, width, height)
        return bitmap
    }
}
