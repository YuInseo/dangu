package com.dangu.modes

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/** 재부팅·업데이트 뒤 버튼을 다시 띄운다(켜 두었을 때만). */
class BootReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (Store.get(context).value.enabled) runCatching { OverlayService.start(context) }
    }
}
