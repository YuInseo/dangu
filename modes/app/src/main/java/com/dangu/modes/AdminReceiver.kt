package com.dangu.modes

import android.app.admin.DeviceAdminReceiver
import android.app.admin.DevicePolicyManager
import android.content.ComponentName
import android.content.Context

/** (고급) Device Owner로 지정됐을 때만 쓰인다 — 잠금 작업 허용 목록을 정하는 데. */
class AdminReceiver : DeviceAdminReceiver() {
    companion object {
        fun component(context: Context) = ComponentName(context, AdminReceiver::class.java)

        fun isDeviceOwner(context: Context): Boolean =
            context.getSystemService(DevicePolicyManager::class.java).isDeviceOwnerApp(context.packageName)
    }
}
