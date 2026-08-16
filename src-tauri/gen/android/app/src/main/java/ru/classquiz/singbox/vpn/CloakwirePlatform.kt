package ru.classquiz.singbox.vpn

import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.VpnService
import android.os.Build
import android.util.Log
import io.nekohasekai.libbox.BridgeOptions
import io.nekohasekai.libbox.BridgeSession
import io.nekohasekai.libbox.ConnectionOwner
import io.nekohasekai.libbox.InterfaceUpdateListener
import io.nekohasekai.libbox.Libbox
import io.nekohasekai.libbox.LocalDNSTransport
import io.nekohasekai.libbox.NeighborUpdateListener
import io.nekohasekai.libbox.NetworkInterfaceIterator
import io.nekohasekai.libbox.Notification
import io.nekohasekai.libbox.PlatformInterface
import io.nekohasekai.libbox.PlatformUser
import io.nekohasekai.libbox.ShellSession
import io.nekohasekai.libbox.StringIterator
import io.nekohasekai.libbox.TunOptions
import io.nekohasekai.libbox.WIFIState
import java.net.InetSocketAddress
import java.util.Collections

/**
 * Android implementation of libbox's [PlatformInterface] — the bridge
 * between the Go core and the OS. The three methods that do real work:
 *
 *  - [openTun]: turns the config's tun inbound into a
 *    [android.net.VpnService.Builder] and returns the tunnel fd;
 *  - [autoDetectInterfaceControl]: `protect()`s outbound sockets so
 *    the core's own traffic bypasses the tunnel (no routing loop);
 *  - [findConnectionOwner]: maps a tunnelled connection back to the
 *    owning UID/package — this is what makes `package_name` route
 *    rules (per-app routing level 2) work.
 *
 * Everything else is either a small network-monitor shim or an
 * explicit "not supported on Android" stub for desktop/Apple-only
 * facilities (shell, bridges, Tailscale SSH, system proxy…).
 */
class CloakwirePlatform(private val service: CloakwireVpnService) : PlatformInterface {

  // ---- TUN ----------------------------------------------------------

  override fun openTun(options: TunOptions): Int {
    // VpnService.Builder is a non-static inner class — construct via receiver.
    // gobind getters like getMTU() map to Kotlin property `mTU` (Kotlin only
    // lowercases the first char), so call the Java methods directly instead.
    val builder = service.Builder()
      .setSession("Cloakwire")
      .setMtu(if (options.getMTU() > 0) options.getMTU() else 9000)
      // Keep the Android TUN descriptor blocking. libbox duplicates this FD
      // before starting the system stack, so O_NONBLOCK would be inherited.
      // The upstream sing-box Android service uses Builder's blocking default.
      .setConfigureIntent(service.mainActivityPendingIntent())

    // Interface addresses from the tun inbound (`address` field).
    val inet4 = options.inet4Address
    while (inet4 != null && inet4.hasNext()) {
      val prefix = inet4.next()
      builder.addAddress(prefix.address(), prefix.prefix())
    }
    val inet6 = options.inet6Address
    while (inet6 != null && inet6.hasNext()) {
      val prefix = inet6.next()
      builder.addAddress(prefix.address(), prefix.prefix())
    }

    // Routes: VpnService needs catch-all routes in both normal and include
    // modes. addAllowedApplication() limits which apps may use those routes;
    // it does not create routes for them. Packaged libbox can provide empty
    // inet4RouteAddress/inet6RouteAddress, so do not rely on those iterators.
    val include = options.includePackage.toList()
    for ((address, prefix) in catchAllRoutes(include.size)) {
      builder.addRoute(address, prefix)
    }

    // DNS servers the OS should associate with the VPN link. Traffic
    // to them is still routed by sing-box rules — this is mostly so
    // Android treats the tunnel as a working network.
    var dnsAdded = false
    try {
      val dns = options.getDNSServerAddress()
      while (dns != null && dns.hasNext()) {
        val server = dns.next()
        runCatching { builder.addDnsServer(server) }.onSuccess { dnsAdded = true }
      }
    } catch (e: Exception) {
      Log.w(TAG, "reading DNS servers from tun options failed", e)
    }
    if (!dnsAdded) {
      builder.addDnsServer("1.1.1.1")
      builder.addDnsServer("8.8.8.8")
    }

    // Per-app routing, system level: include = only these apps enter
    // the tunnel; exclude = everything except these. Our own package
    // is never tunnelled in exclude mode (the core dials via protected
    // sockets anyway, and this keeps the webview/clash traffic direct).
    val includePkgs = options.includePackage.toList()
    if (includePkgs.isNotEmpty()) {
      for (pkg in includePkgs) {
        try {
          builder.addAllowedApplication(pkg)
        } catch (e: PackageManager.NameNotFoundException) {
          Log.w(TAG, "include: package not installed: $pkg")
        }
      }
    } else {
      val exclude = options.excludePackage.toList()
      for (pkg in exclude) {
        try {
          builder.addDisallowedApplication(pkg)
        } catch (e: PackageManager.NameNotFoundException) {
          Log.w(TAG, "exclude: package not installed: $pkg")
        }
      }
      try {
        builder.addDisallowedApplication(service.packageName)
      } catch (e: PackageManager.NameNotFoundException) {
        // Can't happen (we are installed) — ignore.
      }
    }

    val pfd = builder.establish()
      ?: throw Exception("VpnService.Builder.establish() returned null — VPN permission revoked?")
    service.onTunEstablished(pfd)
    // Hand ownership of the fd to the Go core.
    return pfd.detachFd()
  }

  // ---- Outbound socket protection -----------------------------------

  override fun autoDetectInterfaceControl(fd: Int) {
    val protected = service.protect(fd)
    Log.i(TAG, "protect(fd=$fd)=$protected")
    if (!protected) {
      throw Exception("VpnService.protect($fd) failed")
    }
  }

  override fun usePlatformAutoDetectInterfaceControl(): Boolean = true

  // ---- Connection owner (per-app routing level 2) -------------------

  override fun findConnectionOwner(
    ipProtocol: Int,
    sourceAddress: String,
    sourcePort: Int,
    destinationAddress: String,
    destinationPort: Int
  ): ConnectionOwner {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      throw Exception("package_name rules require Android 10+ (connection owner lookup)")
    }
    val cm = service.getSystemService(ConnectivityManager::class.java)
      ?: throw Exception("ConnectivityManager unavailable")
    // Hidden @SystemApi (API 29+). Reflection is the only way to reach
    // it from a non-system app; every sing-box Android client does this.
    val method = ConnectivityManager::class.java.getMethod(
      "getConnectionOwnerUid",
      Int::class.java,
      InetSocketAddress::class.java,
      InetSocketAddress::class.java
    )
    val uid = runCatching {
      method.invoke(
        cm, ipProtocol,
        InetSocketAddress(sourceAddress, sourcePort),
        InetSocketAddress(destinationAddress, destinationPort)
      ) as Int
    }.getOrElse { throw Exception("connection owner lookup failed: ${it.message}") }
    if (uid <= 0) throw Exception("no owner for $sourceAddress:$sourcePort")

    val owner = ConnectionOwner()
    owner.userId = uid
    val packages = service.packageManager.getPackagesForUid(uid)
    owner.setAndroidPackageNames(SimpleStringIterator(packages?.toList() ?: emptyList()))
    return owner
  }

  // ---- Default interface monitor ------------------------------------

  private var defaultInterfaceCallback: ConnectivityManager.NetworkCallback? = null

  override fun startDefaultInterfaceMonitor(listener: InterfaceUpdateListener) {
    val cm = service.getSystemService(ConnectivityManager::class.java) ?: return
    val callback = object : ConnectivityManager.NetworkCallback() {
      override fun onAvailable(network: Network) = push(listener, network)
      override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) =
        push(listener, network)
      override fun onLinkPropertiesChanged(
        network: Network,
        props: android.net.LinkProperties
      ) = push(listener, network)

      override fun onLost(network: Network) {
        // Fall back to whatever non-VPN network is still up.
        currentPhysicalNetwork()?.let { push(listener, it) }
      }

      private fun push(listener: InterfaceUpdateListener, network: Network) {
        val cm = service.getSystemService(ConnectivityManager::class.java) ?: return
        val caps = cm.getNetworkCapabilities(network) ?: return
        // Never report the tunnel itself as the default interface — the core
        // would route its own traffic into the TUN and every dial fails with
        // "no available network interface". (The request below already asks
        // for NOT_VPN networks; this is a belt-and-braces check.)
        if (caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) return
        val props = cm.getLinkProperties(network) ?: return
        val name = props.interfaceName ?: return
        val index = runCatching {
          java.net.NetworkInterface.getByName(name)?.index ?: -1
        }.getOrDefault(-1)
        val expensive = !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
        listener.updateDefaultInterface(name, index, expensive, false)
      }
    }
    // Track only real underlying networks (Wi-Fi/cellular). Without
    // NET_CAPABILITY_NOT_VPN the default-network callback switches to our
    // own tunnel as soon as it is established — routing loops into itself.
    val request = NetworkRequest.Builder()
      .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
      .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
      .build()
    cm.registerNetworkCallback(request, callback)
    defaultInterfaceCallback = callback
    // Push the current state immediately so the core doesn't wait for
    // the first change event.
    currentPhysicalNetwork()?.let { network ->
      val props = cm.getLinkProperties(network)
      val name = props?.interfaceName
      if (name != null) {
        val index = runCatching {
          java.net.NetworkInterface.getByName(name)?.index ?: -1
        }.getOrDefault(-1)
        listener.updateDefaultInterface(name, index, false, false)
      }
    }
  }

  /** First network that has INTERNET and is not a VPN (i.e. the real uplink). */
  private fun currentPhysicalNetwork(): Network? {
    val cm = service.getSystemService(ConnectivityManager::class.java) ?: return null
    return cm.allNetworks.firstOrNull { n ->
      val caps = cm.getNetworkCapabilities(n)
      caps != null &&
        caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
        caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_VPN)
    }
  }

  override fun closeDefaultInterfaceMonitor(listener: InterfaceUpdateListener) {
    val cm = service.getSystemService(ConnectivityManager::class.java) ?: return
    defaultInterfaceCallback?.let { runCatching { cm.unregisterNetworkCallback(it) } }
    defaultInterfaceCallback = null
  }

  // ---- Interface enumeration ----------------------------------------

  override fun getInterfaces(): NetworkInterfaceIterator {
    val cm = service.getSystemService(ConnectivityManager::class.java)
      ?: return NetworkInterfaceList(emptyList())
    val systemInterfaces = runCatching {
      Collections.list(java.net.NetworkInterface.getNetworkInterfaces())
    }.getOrDefault(emptyList())
    val out = mutableListOf<io.nekohasekai.libbox.NetworkInterface>()

    // NetworkInterface alone has no Android transport, DNS, or metering data.
    // Build the libbox view from ConnectivityManager networks, then join the
    // corresponding Java interface for its index, addresses and MTU. This is
    // essential for route.auto_detect_interface to select the physical uplink.
    for (network in cm.allNetworks) {
      runCatching {
        val props = cm.getLinkProperties(network) ?: return@runCatching
        val caps = cm.getNetworkCapabilities(network) ?: return@runCatching
        if (caps.hasTransport(NetworkCapabilities.TRANSPORT_VPN)) return@runCatching
        val name = props.interfaceName ?: return@runCatching
        val ni = systemInterfaces.firstOrNull { it.name == name } ?: return@runCatching

        val item = io.nekohasekai.libbox.NetworkInterface()
        item.name = name
        item.index = ni.index
        item.setMTU(runCatching { ni.mtu }.getOrDefault(0))
        item.addresses = SimpleStringIterator(
          ni.interfaceAddresses
            ?.filter { it.address != null }
            // Android includes a zone suffix on link-local IPv6 addresses;
            // libbox expects plain CIDR prefixes and rejects zoned values.
            ?.map { "${it.address.hostAddress?.substringBefore('%')}/${it.networkPrefixLength}" }
            ?: emptyList()
        )

        var flags = 0
        if (caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)) flags = flags or 0x1
        if (ni.isLoopback) flags = flags or 0x8
        if (ni.isPointToPoint) flags = flags or 0x10
        if (runCatching { ni.supportsMulticast() }.getOrDefault(false)) flags = flags or 0x4
        item.flags = flags
        item.type = when {
          caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> Libbox.InterfaceTypeWIFI
          caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> Libbox.InterfaceTypeCellular
          caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> Libbox.InterfaceTypeEthernet
          else -> Libbox.InterfaceTypeOther
        }
        item.setDNSServer(SimpleStringIterator(
          props.dnsServers.mapNotNull { it.hostAddress?.substringBefore('%') }
        ))
        item.metered = !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
        out.add(item)
      }.onFailure { error ->
        Log.w(TAG, "failed to describe Android network interface", error)
      }
    }
    return NetworkInterfaceList(out)
  }

  private class NetworkInterfaceList(
    private val items: List<io.nekohasekai.libbox.NetworkInterface>
  ) : NetworkInterfaceIterator {
    private var index = 0
    override fun hasNext(): Boolean = index < items.size
    override fun next(): io.nekohasekai.libbox.NetworkInterface = items[index++]
  }

  override fun includeAllNetworks(): Boolean = false

  // ---- DNS ----------------------------------------------------------

  // Returning null makes libbox fall back to its built-in resolver for
  // the `local` DNS transport; our generated configs use explicit
  // udp/https server types which don't need the platform transport.
  override fun localDNSTransport(): LocalDNSTransport? = null

  override fun clearDNSCache() {}

  // ---- WiFi ---------------------------------------------------------

  override fun readWIFIState(): WIFIState? = null

  // ---- Desktop / Apple-only facilities: explicit stubs --------------

  override fun checkPlatformShell() {
    throw Exception("platform shell is not supported on Android")
  }

  override fun openShellSession(
    user: PlatformUser?,
    shell: String?,
    args: StringIterator?,
    env: String?,
    rows: Int,
    cols: Int
  ): ShellSession {
    throw Exception("platform shell is not supported on Android")
  }

  override fun usePlatformShell(): Boolean = false

  override fun createBridge(options: BridgeOptions?): BridgeSession {
    throw Exception("platform bridge is not supported on Android")
  }

  override fun usePlatformBridge(): Boolean = false

  override fun lookupUser(username: String?): PlatformUser {
    throw Exception("user lookup is not supported on Android")
  }

  override fun lookupSFTPServer(): String {
    throw Exception("SFTP is not supported on Android")
  }

  override fun readSystemSSHHostKey(): String {
    throw Exception("SSH host keys are not supported on Android")
  }

  override fun tailscaleHostname(): String = ""

  override fun registerMyInterface(name: String?) {}

  override fun underNetworkExtension(): Boolean = false

  override fun useProcFS(): Boolean = false

  override fun sendNotification(notification: Notification?) {
    // The service keeps its own foreground notification; core-issued
    // notifications (route changes etc.) are not surfaced in v1.
  }

  override fun startNeighborMonitor(listener: NeighborUpdateListener?) {
    throw Exception("neighbor monitor is not supported on Android")
  }

  override fun closeNeighborMonitor(listener: NeighborUpdateListener?) {}

  companion object {
    internal fun catchAllRoutes(@Suppress("UNUSED_PARAMETER") includePackageCount: Int): List<Pair<String, Int>> =
      listOf("0.0.0.0" to 0, "::" to 0)

    private const val TAG = "CloakwirePlatform"
  }
}
