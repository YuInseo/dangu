package com.donghaeng.app

import android.graphics.BitmapFactory
import android.util.Base64
import com.getcapacitor.JSArray
import com.getcapacitor.JSObject
import com.getcapacitor.Plugin
import com.getcapacitor.PluginCall
import com.getcapacitor.PluginMethod
import com.getcapacitor.annotation.CapacitorPlugin
import com.google.mlkit.vision.common.InputImage
import com.google.mlkit.vision.text.TextRecognition
import com.google.mlkit.vision.text.korean.KoreanTextRecognizerOptions

/**
 * 승차권 사진에서 글자를 읽는다 — 구글 ML Kit의 한국어 인식기로, 기기 안에서.
 *
 * 서버로 사진을 보내지 않는다. 표에는 고객 이름이 찍혀 있는 경우가 있고, 승강장은 신호가
 * 약하다. 인식기는 APK 안에 들어 있어서(app.gradle) 처음 쓸 때 내려받을 것도 없다.
 *
 * 글자를 해석하는 일(어느 숫자가 열차번호인지)은 하지 않는다 — 그건 자바스크립트
 * (`lib/ticket.ts`)가 하고, 여기서는 줄마다 글자와 자리만 돌려준다. 해석 규칙은 자주
 * 고치게 되는데, 그쪽에 있으면 APK 없이 조용한 업데이트로 고칠 수 있다.
 */
@CapacitorPlugin(name = "TicketOcr")
class TicketOcrPlugin : Plugin() {
    private val recognizer by lazy {
        TextRecognition.getClient(KoreanTextRecognizerOptions.Builder().build())
    }

    /**
     * @returns text: String, lines: Array
     */
    @PluginMethod
    fun recognize(call: PluginCall) {
        val data = call.getString("base64") ?: return call.reject("사진이 없습니다")
        val bytes = try {
            Base64.decode(data.substringAfter(","), Base64.DEFAULT)
        } catch (e: IllegalArgumentException) {
            return call.reject("사진을 읽지 못했습니다")
        }
        val bitmap = BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
            ?: return call.reject("사진을 읽지 못했습니다")

        recognizer.process(InputImage.fromBitmap(bitmap, 0))
            .addOnSuccessListener { result ->
                val lines = JSArray()
                for (block in result.textBlocks) {
                    for (line in block.lines) {
                        val box = line.boundingBox
                        val entry = JSObject()
                        entry.put("text", line.text)
                        entry.put("top", box?.top ?: 0)
                        entry.put("left", box?.left ?: 0)
                        entry.put("height", box?.height() ?: 0)
                        lines.put(entry)
                    }
                }
                val out = JSObject()
                out.put("text", result.text)
                out.put("lines", lines)
                call.resolve(out)
            }
            .addOnFailureListener { error ->
                call.reject(error.message ?: "글자를 읽지 못했습니다")
            }
    }
}
