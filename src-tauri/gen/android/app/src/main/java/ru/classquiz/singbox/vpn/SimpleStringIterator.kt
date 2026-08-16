package ru.classquiz.singbox.vpn

import io.nekohasekai.libbox.StringIterator

/**
 * gomobile exposes Go string slices as a pull-based [StringIterator].
 * This adapts a plain Kotlin list to that interface (used to hand
 * package-name lists and address strings back to the Go core).
 */
class SimpleStringIterator(private val items: List<String>) : StringIterator {
  private var index = 0

  override fun hasNext(): Boolean = index < items.size

  override fun len(): Int = items.size

  override fun next(): String = items[index++]
}

/** Drain a nullable gomobile [StringIterator] into a Kotlin list. */
fun StringIterator?.toList(): List<String> {
  if (this == null) return emptyList()
  val out = ArrayList<String>(len().coerceAtLeast(0))
  while (hasNext()) out.add(next())
  return out
}
