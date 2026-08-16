package ru.classquiz.singbox.vpn

import org.junit.Assert.assertEquals
import org.junit.Test

class CloakwirePlatformRoutingPolicyTest {
  @Test
  fun `installs catch-all routes when include mode is active`() {
    assertEquals(
      listOf("0.0.0.0" to 0, "::" to 0),
      CloakwirePlatform.catchAllRoutes(includePackageCount = 1)
    )
  }
}
