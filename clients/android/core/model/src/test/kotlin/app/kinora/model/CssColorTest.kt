package app.kinora.model

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CssColorTest {
    private fun hex(c: Int?) = c?.let { "%08X".format(it) }

    @Test
    fun hexForms() {
        assertEquals("FF0B1612", hex(parseCssColor("#0B1612")))
        assertEquals("FFFFFFFF", hex(parseCssColor("#fff")))
        assertEquals("80FF0000", hex(parseCssColor("#ff000080")))
        assertEquals("CCAABBDD", hex(parseCssColor("#abdc")))
    }

    @Test
    fun rgbAndHsl() {
        assertEquals("FF2BD48F", hex(parseCssColor("rgb(43, 212, 143)")))
        assertEquals("802BD48F", hex(parseCssColor("rgba(43 212 143 / 50%)")))
        assertEquals("FFFF0000", hex(parseCssColor("hsl(0 100% 50%)")))
        assertEquals("FF00FF00", hex(parseCssColor("hsl(120deg, 100%, 50%)")))
    }

    @Test
    fun oklch() {
        assertEquals("FFFFFFFF", hex(parseCssColor("oklch(1 0 0)")))
        assertEquals("FF000000", hex(parseCssColor("oklch(0% 0 0)")))
        // Pure sRGB red is oklch(0.628 0.2577 29.23).
        assertEquals("FFFF0000", hex(parseCssColor("oklch(0.628 0.2577 29.23)")))
    }

    @Test
    fun rejectsJunk() {
        assertNull(parseCssColor("red"))
        assertNull(parseCssColor("#12"))
        assertNull(parseCssColor("rgb(1,2)"))
        assertNull(parseCssColor("url(evil)"))
    }

    @Test
    fun lengths() {
        assertEquals(12f, parseCssLengthDp("0.75rem"))
        assertEquals(8f, parseCssLengthDp("8px"))
        assertNull(parseCssLengthDp("big"))
    }
}
