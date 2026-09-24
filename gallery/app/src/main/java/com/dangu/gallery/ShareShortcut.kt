package com.dangu.gallery

import android.content.Context
import android.content.Intent
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat

/**
 * 공유 창 맨 윗줄의 "사진 정보".
 *
 * 다른 갤러리의 선택 막대(만들기·공유·삭제·더보기)에는 다른 앱이 단추를 넣을 수 없다.
 * 그 대신 공유 창의 바로 공유 줄에 올려 두면 "고르기 → 공유 → 사진 정보" 두 번으로 닿는다.
 * `res/xml/shortcuts.xml`의 share-target과 같은 카테고리로 묶여야 공유 창에 뜬다.
 */
object ShareShortcut {
    private const val ID = "photo_info"
    private const val CATEGORY = "com.dangu.gallery.category.PHOTO_INFO"

    fun publish(context: Context) {
        runCatching {
            val shortcut = ShortcutInfoCompat.Builder(context, ID)
                .setShortLabel(context.getString(R.string.info_name))
                .setLongLabel("사진 정보 보기")
                .setIcon(IconCompat.createWithResource(context, R.mipmap.ic_info_launcher))
                .setIntent(Intent(context, InfoActivity::class.java).setAction(Intent.ACTION_MAIN))
                .setCategories(setOf(CATEGORY))
                .setLongLived(true)
                .setRank(0)
                .build()
            ShortcutManagerCompat.pushDynamicShortcut(context, shortcut)
        }
    }
}
