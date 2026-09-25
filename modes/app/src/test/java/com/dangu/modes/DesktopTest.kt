package com.dangu.modes

import com.dangu.modes.Desktop.Companion.COLS
import com.dangu.modes.Desktop.Companion.DOCK_PAGE
import com.dangu.modes.Desktop.Companion.ROWS
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DesktopTest {
    private val a = DItem.App("a")
    private val b = DItem.App("b")
    private val c = DItem.App("c")

    @Test fun startsEmpty() {
        val d = Desktop()
        assertTrue(d.isEmpty)
        assertEquals(1, d.pages.size)
    }

    @Test fun moveToEmptyCell() {
        val d = Desktop().put(Slot(0, 0), a).drop(Slot(0, 0), Slot(0, 5))
        assertNull(d.at(Slot(0, 0)))
        assertEquals(a, d.at(Slot(0, 5)))
    }

    @Test fun appOnAppMakesFolder() {
        val d = Desktop().put(Slot(0, 0), a).put(Slot(0, 1), b).drop(Slot(0, 0), Slot(0, 1))
        val f = d.at(Slot(0, 1)) as DItem.Folder
        assertEquals(listOf("b", "a"), f.apps)
        assertNull(d.at(Slot(0, 0)))
    }

    @Test fun appOnFolderJoinsIt() {
        var d = Desktop().put(Slot(0, 0), a).put(Slot(0, 1), b).drop(Slot(0, 0), Slot(0, 1))
        d = d.put(Slot(0, 2), c).drop(Slot(0, 2), Slot(0, 1))
        assertEquals(listOf("b", "a", "c"), (d.at(Slot(0, 1)) as DItem.Folder).apps)
    }

    @Test fun folderOnAppSwaps() {
        var d = Desktop().put(Slot(0, 0), a).put(Slot(0, 1), b).drop(Slot(0, 0), Slot(0, 1))
        d = d.put(Slot(0, 3), c).drop(Slot(0, 1), Slot(0, 3))
        assertEquals(c, d.at(Slot(0, 1)))
        assertTrue(d.at(Slot(0, 3)) is DItem.Folder)
    }

    @Test fun toDockAndAcrossPages() {
        var d = Desktop().addPage().put(Slot(0, 0), a)
        d = d.drop(Slot(0, 0), Slot(DOCK_PAGE, 2))
        assertEquals(a, d.dock[2])
        d = d.drop(Slot(DOCK_PAGE, 2), Slot(1, 7))
        assertEquals(a, d.at(Slot(1, 7)))
        assertTrue(d.dock.isEmpty())
    }

    @Test fun addAppsFillsAndOverflows() {
        val many = (0 until COLS * ROWS + 3).map { "p$it" }
        val d = Desktop().addApps(many + "p0")
        assertEquals(2, d.pages.size)
        assertEquals(COLS * ROWS, d.pages[0].size)
        assertEquals(3, d.pages[1].size)
        assertEquals(many.size, d.allApps().size) // 중복은 안 넣는다
    }

    @Test fun takeOutOfFolderUnwrapsLastOne() {
        var d = Desktop().put(Slot(0, 0), a).put(Slot(0, 1), b).drop(Slot(0, 0), Slot(0, 1))
        d = d.takeOutOfFolder(Slot(0, 1), "a")
        assertEquals(b, d.at(Slot(0, 1)))
        assertTrue("a" in d.allApps())
    }

    @Test fun removePageKeepsAtLeastOne() {
        val d = Desktop().removePage(0)
        assertEquals(1, d.pages.size)
        assertEquals(1, Desktop().addPage().removePage(1).pages.size)
    }

    @Test fun jsonRoundTrip() {
        var d = Desktop().addPage().put(Slot(0, 0), a).put(Slot(0, 1), b).drop(Slot(0, 0), Slot(0, 1))
        d = d.renameFolder(Slot(0, 1), "게임").put(Slot(1, 4), c).put(Slot(DOCK_PAGE, 0), DItem.App("phone"))
        val back = Desktop.fromJson(d.toJson())
        assertEquals(d, back)
        assertEquals("게임", (back.at(Slot(0, 1)) as DItem.Folder).name)
    }

    @Test fun brokenJsonGivesEmpty() {
        assertTrue(Desktop.fromJson("{not json").isEmpty)
        assertTrue(Desktop.fromJson(null).isEmpty)
    }
}
