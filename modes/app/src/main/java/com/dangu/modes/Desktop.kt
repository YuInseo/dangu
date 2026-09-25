package com.dangu.modes

import org.json.JSONArray
import org.json.JSONObject

/**
 * 새 바탕화면(비밀 바탕화면)의 배치. 처음엔 비어 있고, 편집 모드에서 채운다.
 * 한 페이지는 [COLS]×[ROWS] 칸, 아래 독은 [DOCK] 칸.
 */
sealed interface DItem {
    val key: String

    data class App(val pkg: String) : DItem {
        override val key get() = "a:$pkg"
    }

    data class Folder(val id: String, val name: String, val apps: List<String>) : DItem {
        override val key get() = "f:$id"
    }
}

/** 칸 위치. page == DOCK_PAGE면 독. */
data class Slot(val page: Int, val cell: Int)

data class Desktop(
    /** 페이지마다 칸 번호 → 항목 */
    val pages: List<Map<Int, DItem>> = listOf(emptyMap()),
    val dock: Map<Int, DItem> = emptyMap(),
) {
    val isEmpty get() = pages.all { it.isEmpty() } && dock.isEmpty()

    fun at(slot: Slot): DItem? = if (slot.page == DOCK_PAGE) dock[slot.cell] else pages.getOrNull(slot.page)?.get(slot.cell)

    private fun set(slot: Slot, item: DItem?): Desktop =
        if (slot.page == DOCK_PAGE) copy(dock = dock.toMutableMap().apply { if (item == null) remove(slot.cell) else put(slot.cell, item) })
        else copy(pages = pages.mapIndexed { i, p ->
            if (i != slot.page) p else p.toMutableMap().apply { if (item == null) remove(slot.cell) else put(slot.cell, item) }
        })

    fun put(slot: Slot, item: DItem) = set(slot, item)
    fun remove(slot: Slot) = set(slot, null)

    /** 끌어다 놓기. 빈 칸이면 옮기고, 앱 위면 폴더를 만들고, 폴더 위면 넣고, 같은 자리면 그대로. */
    fun drop(from: Slot, to: Slot): Desktop {
        if (from == to) return this
        val moving = at(from) ?: return this
        val target = at(to)
        return when {
            target == null -> remove(from).put(to, moving)
            moving is DItem.App && target is DItem.App ->
                remove(from).put(to, DItem.Folder(Mode.newId(), "폴더", listOf(target.pkg, moving.pkg)))
            moving is DItem.App && target is DItem.Folder ->
                remove(from).put(to, target.copy(apps = (target.apps + moving.pkg).distinct()))
            // 폴더를 무엇 위에 놓으면 자리를 바꾼다.
            else -> put(from, target).put(to, moving)
        }
    }

    /** 앱들을 빈 칸에 차례로. 페이지가 모자라면 새로 만든다. 이미 있는 앱은 건너뛴다. */
    fun addApps(pkgs: List<String>, startPage: Int = 0): Desktop {
        var d = this
        // 넣을 때마다 늘려 간다 — 한 번에 같은 앱이 두 번 와도 한 번만.
        val present = allApps().toMutableSet()
        for (pkg in pkgs) {
            if (!present.add(pkg)) continue
            var placed = false
            for (p in (startPage until d.pages.size) + (0 until startPage)) {
                val free = (0 until COLS * ROWS).firstOrNull { it !in d.pages[p] }
                if (free != null) { d = d.put(Slot(p, free), DItem.App(pkg)); placed = true; break }
            }
            if (!placed) d = d.copy(pages = d.pages + listOf(mapOf(0 to DItem.App(pkg))))
        }
        return d
    }

    fun allApps(): Set<String> =
        (pages.flatMap { it.values } + dock.values).flatMap { if (it is DItem.App) listOf(it.pkg) else (it as DItem.Folder).apps }.toSet()

    fun addPage() = copy(pages = pages + listOf(emptyMap()))

    fun removePage(i: Int): Desktop =
        if (pages.size <= 1) this else copy(pages = pages.filterIndexed { idx, _ -> idx != i })

    /** 폴더 안의 앱을 빼서 빈 칸으로. 폴더에 하나만 남으면 폴더를 풀어 그 앱만 둔다. */
    fun takeOutOfFolder(slot: Slot, pkg: String): Desktop {
        val f = at(slot) as? DItem.Folder ?: return this
        val rest = f.apps - pkg
        val d = when (rest.size) {
            0 -> remove(slot)
            1 -> put(slot, DItem.App(rest[0]))
            else -> put(slot, f.copy(apps = rest))
        }
        return d.addApps(listOf(pkg), if (slot.page == DOCK_PAGE) 0 else slot.page)
    }

    fun renameFolder(slot: Slot, name: String): Desktop {
        val f = at(slot) as? DItem.Folder ?: return this
        return put(slot, f.copy(name = name.ifBlank { "폴더" }))
    }

    fun toJson(): String {
        fun item(it: DItem) = when (it) {
            is DItem.App -> JSONObject().put("t", "app").put("pkg", it.pkg)
            is DItem.Folder -> JSONObject().put("t", "folder").put("id", it.id).put("name", it.name).put("apps", JSONArray(it.apps))
        }
        fun cells(m: Map<Int, DItem>) = JSONObject().apply { m.forEach { (k, v) -> put(k.toString(), item(v)) } }
        return JSONObject()
            .put("pages", JSONArray().apply { pages.forEach { put(cells(it)) } })
            .put("dock", cells(dock))
            .toString()
    }

    companion object {
        const val COLS = 4
        const val ROWS = 5
        const val DOCK = 5
        const val DOCK_PAGE = -1

        fun fromJson(json: String?): Desktop = runCatching {
            val o = JSONObject(json ?: return Desktop())
            fun item(j: JSONObject): DItem? = when (j.optString("t")) {
                "app" -> DItem.App(j.getString("pkg"))
                "folder" -> DItem.Folder(
                    j.getString("id"), j.optString("name", "폴더"),
                    j.getJSONArray("apps").let { a -> (0 until a.length()).map { a.getString(it) } },
                )
                else -> null
            }
            fun cells(j: JSONObject?): Map<Int, DItem> =
                j?.keys()?.asSequence()?.mapNotNull { k -> item(j.getJSONObject(k))?.let { k.toInt() to it } }?.toMap() ?: emptyMap()
            val pa = o.optJSONArray("pages")
            val pages = if (pa == null) emptyList() else (0 until pa.length()).map { cells(pa.getJSONObject(it)) }
            Desktop(pages.ifEmpty { listOf(emptyMap()) }, cells(o.optJSONObject("dock")))
        }.getOrDefault(Desktop())
    }
}
