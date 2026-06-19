import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';
import 'dart:ui';

import 'package:cryptography/cryptography.dart';
import 'package:flutter/material.dart';
import 'package:record/record.dart';

import 'stream_protocol.dart';

const _defaultPort = 49000;
const _defaultSampleRate = 48000;
const _defaultChannels = 1;

void main() {
  runApp(const MicOWirelessApp());
}

class MicOWirelessApp extends StatelessWidget {
  const MicOWirelessApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'micOwireless Mobile',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF6B7CFF)),
      ),
      home: const StreamControlPage(),
    );
  }
}

class StreamControlPage extends StatefulWidget {
  const StreamControlPage({super.key});

  @override
  State<StreamControlPage> createState() => _StreamControlPageState();
}

class _StreamControlPageState extends State<StreamControlPage> {
  final _recorder = AudioRecorder();
  final _targetIpController = TextEditingController(text: '192.168.1.100');
  final _portController = TextEditingController(text: '$_defaultPort');
  final _pairCodeController = TextEditingController(text: _generatePairCode());

  StreamSubscription<Uint8List>? _audioSubscription;
  RawDatagramSocket? _udpSocket;
  Future<void> _sendQueue = Future<void>.value();

  bool _isStreaming = false;
  bool _discovering = false;
  bool _secureMode = true;
  String _statusMessage = 'Ready to stream';
  String? _errorMessage;
  int _packetsSent = 0;
  DateTime? _streamingSince;
  int _sessionId = 0;
  List<_DiscoveredDesktop> _discoveredDesktops = const [];

  @override
  void dispose() {
    _audioSubscription?.cancel();
    _udpSocket?.close();
    _recorder.dispose();
    _targetIpController.dispose();
    _portController.dispose();
    _pairCodeController.dispose();
    super.dispose();
  }

  Future<void> _toggleStreaming() async {
    if (_isStreaming) {
      await _stopStreaming();
      return;
    }
    await _startStreaming();
  }

  Future<void> _discoverDesktops() async {
    final port = int.tryParse(_portController.text.trim());
    if (port == null || port < 1 || port > 65534) {
      setState(() => _errorMessage = 'Use a valid port between 1 and 65534.');
      return;
    }

    setState(() {
      _discovering = true;
      _errorMessage = null;
      _discoveredDesktops = const [];
      _statusMessage = 'Scanning local network for desktops...';
    });

    final discovered = <String, _DiscoveredDesktop>{};
    RawDatagramSocket? socket;
    StreamSubscription<RawSocketEvent>? subscription;

    try {
      socket = await RawDatagramSocket.bind(InternetAddress.anyIPv4, 0);
      socket.broadcastEnabled = true;
      subscription = socket.listen((event) {
        if (event != RawSocketEvent.read) {
          return;
        }
        Datagram? datagram;
        while ((datagram = socket!.receive()) != null) {
          _handleDiscoveryDatagram(datagram!, discovered);
        }
      });

      final request = utf8.encode(
        jsonEncode(const {
          'kind': discoveryRequestKind,
          'protocol': protocolName,
        }),
      );
      socket.send(
        request,
        InternetAddress('255.255.255.255'),
        port + discoveryPortOffset,
      );

      await Future<void>.delayed(const Duration(milliseconds: 1800));
    } catch (error) {
      if (!mounted) return;
      setState(() {
        _errorMessage = 'Desktop discovery failed: $error';
      });
    } finally {
      await subscription?.cancel();
      socket?.close();
    }

    if (!mounted) return;
    final desktops = discovered.values.toList()
      ..sort((left, right) => left.name.compareTo(right.name));
    setState(() {
      _discovering = false;
      _discoveredDesktops = desktops;
      _statusMessage = desktops.isEmpty
          ? 'No desktop found. Ensure desktop receiver is running first.'
          : 'Found ${desktops.length} desktop(s). Tap one to auto-fill.';
    });
  }

  void _handleDiscoveryDatagram(
    Datagram datagram,
    Map<String, _DiscoveredDesktop> discovered,
  ) {
    try {
      final decoded = jsonDecode(
        utf8.decode(datagram.data, allowMalformed: true),
      );
      if (decoded is! Map<String, dynamic>) {
        return;
      }
      if (decoded['kind'] != discoveryResponseKind ||
          decoded['protocol'] != protocolName) {
        return;
      }

      final portValue = (decoded['port'] as num?)?.toInt();
      if (portValue == null || portValue < 1 || portValue > 65535) {
        return;
      }

      final desktop = _DiscoveredDesktop(
        name: decoded['name']?.toString() ?? 'micOwireless Desktop',
        ip: datagram.address.address,
        port: portValue,
        secureRequired: decoded['secureRequired'] == true,
      );
      discovered['${desktop.ip}:${desktop.port}'] = desktop;
    } catch (_) {
      // Ignore malformed UDP replies from unrelated services.
    }
  }

  Future<void> _startStreaming() async {
    final ip = InternetAddress.tryParse(_targetIpController.text.trim());
    final port = int.tryParse(_portController.text.trim());
    final pairCode = _pairCodeController.text.trim();

    if (ip == null) {
      setState(() => _errorMessage = 'Enter a valid desktop IPv4 address.');
      return;
    }
    if (port == null || port < 1 || port > 65535) {
      setState(() => _errorMessage = 'Port must be between 1 and 65535.');
      return;
    }
    if (_secureMode && pairCode.length < 6) {
      setState(
        () => _errorMessage = 'Pair code must have at least 6 characters.',
      );
      return;
    }

    final hasPermission = await _recorder.hasPermission();
    if (!hasPermission) {
      setState(() => _errorMessage = 'Microphone permission is required.');
      return;
    }

    SecretKey? secureKey;
    if (_secureMode) {
      secureKey = await derivePairingKey(pairCode);
    }

    final random = Random.secure();
    final sessionId = (random.nextInt(1 << 32) << 32) | random.nextInt(1 << 32);
    var packetCounter = 0;
    var nextSequence = 0;

    try {
      final socket = await RawDatagramSocket.bind(InternetAddress.anyIPv4, 0);
      final stream = await _recorder.startStream(
        const RecordConfig(
          encoder: AudioEncoder.pcm16bits,
          sampleRate: _defaultSampleRate,
          numChannels: _defaultChannels,
          autoGain: true,
          echoCancel: true,
          noiseSuppress: true,
        ),
      );

      setState(() {
        _udpSocket = socket;
        _isStreaming = true;
        _sessionId = sessionId;
        _statusMessage = 'Streaming to ${ip.address}:$port';
        _errorMessage = null;
        _packetsSent = 0;
        _streamingSince = DateTime.now();
      });

      _sendQueue = Future<void>.value();
      final subscription = stream.listen(
        (audioChunk) {
          final packetSequence = nextSequence++;
          _sendQueue = _sendQueue
              .then((_) async {
                if (!_isStreaming) {
                  return;
                }
                final packet = await buildAudioPacket(
                  sequence: packetSequence,
                  sessionId: sessionId,
                  sampleRate: _defaultSampleRate,
                  channels: _defaultChannels,
                  pcm16Payload: audioChunk,
                  secureMode: _secureMode,
                  pairingKey: secureKey,
                );
                final sent = socket.send(packet, ip, port);
                if (sent <= 0) {
                  throw StateError('UDP packet could not be sent.');
                }
                packetCounter += 1;
                if (packetCounter % 20 == 0 && mounted) {
                  setState(() => _packetsSent = packetCounter);
                }
              })
              .catchError((Object error) {
                if (!mounted) return;
                setState(() => _errorMessage = 'Streaming failed: $error');
                unawaited(_stopStreaming());
              });
        },
        onError: (Object error) {
          if (!mounted) return;
          setState(() => _errorMessage = 'Streaming failed: $error');
          unawaited(_stopStreaming());
        },
      );

      _audioSubscription = subscription;
    } catch (error) {
      if (await _recorder.isRecording()) {
        await _recorder.stop();
      }
      if (!mounted) return;
      setState(() {
        _errorMessage = 'Could not start streaming: $error';
        _statusMessage = 'Ready to stream';
        _isStreaming = false;
      });
    }
  }

  Future<void> _stopStreaming() async {
    if (!_isStreaming &&
        _audioSubscription == null &&
        _udpSocket == null &&
        !(await _recorder.isRecording())) {
      return;
    }

    if (mounted) {
      setState(() {
        _isStreaming = false;
        _statusMessage = 'Stopping stream...';
      });
    }

    final subscription = _audioSubscription;
    _audioSubscription = null;
    await subscription?.cancel();

    try {
      await _sendQueue;
    } catch (_) {
      // Stop still proceeds even if one packet send failed.
    }

    final socket = _udpSocket;
    _udpSocket = null;
    socket?.close();

    if (await _recorder.isRecording()) {
      await _recorder.stop();
    }

    if (!mounted) return;
    setState(() {
      _statusMessage = 'Ready to stream';
      _streamingSince = null;
      _sessionId = 0;
    });
  }

  void _applyDiscovery(_DiscoveredDesktop desktop) {
    _targetIpController.text = desktop.ip;
    _portController.text = '${desktop.port}';
    if (desktop.secureRequired) {
      setState(() => _secureMode = true);
    }
  }

  static String _generatePairCode() {
    const alphabet = 'ABCDEFGHJKLMNPQRSTUVWXYZ23456789';
    final random = Random.secure();
    return List.generate(
      8,
      (_) => alphabet[random.nextInt(alphabet.length)],
    ).join();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final uptime = _streamingSince == null
        ? const Duration()
        : DateTime.now().difference(_streamingSince!);

    return Scaffold(
      backgroundColor: Colors.transparent,
      body: Stack(
        children: [
          const _GradientBackdrop(),
          SafeArea(
            child: Padding(
              padding: const EdgeInsets.all(20),
              child: SingleChildScrollView(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      'micOwireless',
                      style: theme.textTheme.headlineLarge?.copyWith(
                        color: Colors.white,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                    const SizedBox(height: 8),
                    Text(
                      'Secure wireless microphone stream for your desktop.',
                      style: theme.textTheme.bodyLarge?.copyWith(
                        color: Colors.white.withValues(alpha: 0.9),
                      ),
                    ),
                    const SizedBox(height: 18),
                    _GlassCard(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Expanded(
                                child: Text(
                                  'Connection',
                                  style: theme.textTheme.titleLarge?.copyWith(
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                              ),
                              OutlinedButton.icon(
                                onPressed: _discovering
                                    ? null
                                    : _discoverDesktops,
                                icon: _discovering
                                    ? const SizedBox(
                                        width: 16,
                                        height: 16,
                                        child: CircularProgressIndicator(
                                          strokeWidth: 2,
                                        ),
                                      )
                                    : const Icon(Icons.radar, size: 18),
                                label: const Text('Discover'),
                              ),
                            ],
                          ),
                          const SizedBox(height: 10),
                          TextField(
                            controller: _targetIpController,
                            keyboardType: TextInputType.number,
                            decoration: const InputDecoration(
                              labelText: 'Desktop IP Address',
                              prefixIcon: Icon(Icons.wifi),
                            ),
                          ),
                          const SizedBox(height: 10),
                          TextField(
                            controller: _portController,
                            keyboardType: TextInputType.number,
                            decoration: const InputDecoration(
                              labelText: 'UDP Port',
                              prefixIcon: Icon(Icons.hub),
                            ),
                          ),
                          if (_discoveredDesktops.isNotEmpty) ...[
                            const SizedBox(height: 12),
                            Text(
                              'Discovered desktops',
                              style: theme.textTheme.labelLarge,
                            ),
                            const SizedBox(height: 8),
                            Wrap(
                              spacing: 8,
                              runSpacing: 8,
                              children: _discoveredDesktops
                                  .map(
                                    (desktop) => ActionChip(
                                      avatar: const Icon(
                                        Icons.computer,
                                        size: 16,
                                      ),
                                      label: Text(
                                        '${desktop.name} (${desktop.ip}:${desktop.port})',
                                      ),
                                      onPressed: () => _applyDiscovery(desktop),
                                    ),
                                  )
                                  .toList(),
                            ),
                          ],
                        ],
                      ),
                    ),
                    const SizedBox(height: 12),
                    _GlassCard(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Row(
                            children: [
                              Expanded(
                                child: Text(
                                  'Security & Pairing',
                                  style: theme.textTheme.titleMedium?.copyWith(
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                              ),
                              Switch(
                                value: _secureMode,
                                onChanged: _isStreaming
                                    ? null
                                    : (value) =>
                                          setState(() => _secureMode = value),
                              ),
                            ],
                          ),
                          TextField(
                            controller: _pairCodeController,
                            enabled: !_isStreaming,
                            decoration: InputDecoration(
                              labelText: 'Pair Code',
                              prefixIcon: const Icon(Icons.lock_outline),
                              suffixIcon: IconButton(
                                onPressed: _isStreaming
                                    ? null
                                    : () {
                                        _pairCodeController.text =
                                            _generatePairCode();
                                      },
                                icon: const Icon(Icons.refresh),
                                tooltip: 'Generate new code',
                              ),
                            ),
                          ),
                          const SizedBox(height: 8),
                          Text(
                            _secureMode
                                ? 'Audio packets are encrypted with this pairing code.'
                                : 'Secure mode is OFF. Audio is sent without encryption.',
                            style: theme.textTheme.bodySmall,
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 12),
                    _GlassCard(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          FilledButton.tonalIcon(
                            style: FilledButton.styleFrom(
                              minimumSize: const Size.fromHeight(52),
                            ),
                            onPressed: _toggleStreaming,
                            icon: Icon(_isStreaming ? Icons.stop : Icons.mic),
                            label: Text(
                              _isStreaming
                                  ? 'Stop Streaming'
                                  : 'Start Streaming',
                            ),
                          ),
                          const SizedBox(height: 12),
                          Wrap(
                            spacing: 8,
                            runSpacing: 8,
                            children: [
                              _StatusChip(
                                icon: Icons.circle,
                                label: _isStreaming ? 'Live' : 'Idle',
                                color: _isStreaming
                                    ? const Color(0xFF22C55E)
                                    : theme.colorScheme.primary,
                              ),
                              _StatusChip(
                                icon: Icons.graphic_eq,
                                label: 'Packets: $_packetsSent',
                                color: theme.colorScheme.secondary,
                              ),
                              _StatusChip(
                                icon: Icons.timer_outlined,
                                label: 'Uptime: ${uptime.inSeconds}s',
                                color: theme.colorScheme.tertiary,
                              ),
                              _StatusChip(
                                icon: Icons.shield,
                                label: _secureMode
                                    ? 'Encrypted'
                                    : 'Unencrypted',
                                color: _secureMode
                                    ? const Color(0xFF4F46E5)
                                    : const Color(0xFFB45309),
                              ),
                            ],
                          ),
                          const SizedBox(height: 12),
                          Text(
                            _statusMessage,
                            style: theme.textTheme.bodyMedium,
                          ),
                          if (_sessionId != 0) ...[
                            const SizedBox(height: 4),
                            Text(
                              'Session ID: $_sessionId',
                              style: theme.textTheme.bodySmall,
                            ),
                          ],
                          if (_errorMessage != null) ...[
                            const SizedBox(height: 8),
                            Text(
                              _errorMessage!,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: theme.colorScheme.error,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                    const SizedBox(height: 12),
                    _GlassCard(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            'Audio Profile',
                            style: theme.textTheme.titleMedium?.copyWith(
                              fontWeight: FontWeight.w700,
                            ),
                          ),
                          const SizedBox(height: 8),
                          Text(
                            'Protocol $protocolName • PCM 16-bit • $_defaultSampleRate Hz • Mono • UDP + jitter-safe receiver',
                            style: theme.textTheme.bodyMedium,
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _DiscoveredDesktop {
  const _DiscoveredDesktop({
    required this.name,
    required this.ip,
    required this.port,
    required this.secureRequired,
  });

  final String name;
  final String ip;
  final int port;
  final bool secureRequired;
}

class _GlassCard extends StatelessWidget {
  const _GlassCard({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    return ClipRRect(
      borderRadius: BorderRadius.circular(22),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 18, sigmaY: 18),
        child: Container(
          width: double.infinity,
          padding: const EdgeInsets.all(18),
          decoration: BoxDecoration(
            color: Colors.white.withValues(alpha: 0.64),
            borderRadius: BorderRadius.circular(22),
            border: Border.all(color: Colors.white.withValues(alpha: 0.55)),
            boxShadow: [
              BoxShadow(
                color: Colors.black.withValues(alpha: 0.08),
                blurRadius: 16,
                offset: const Offset(0, 8),
              ),
            ],
          ),
          child: child,
        ),
      ),
    );
  }
}

class _StatusChip extends StatelessWidget {
  const _StatusChip({
    required this.icon,
    required this.label,
    required this.color,
  });

  final IconData icon;
  final String label;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: color.withValues(alpha: 0.13),
        borderRadius: BorderRadius.circular(999),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 14, color: color),
          const SizedBox(width: 6),
          Text(
            label,
            style: Theme.of(context).textTheme.labelMedium?.copyWith(
              fontWeight: FontWeight.w600,
              color: color.withValues(alpha: 0.95),
            ),
          ),
        ],
      ),
    );
  }
}

class _GradientBackdrop extends StatelessWidget {
  const _GradientBackdrop();

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: const BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [Color(0xFF0D1B4C), Color(0xFF3F5BFF), Color(0xFF6B7CFF)],
        ),
      ),
      child: Stack(
        children: [
          Positioned(
            top: -80,
            right: -80,
            child: _GlowBubble(
              size: 260,
              color: Colors.white.withValues(alpha: 0.2),
            ),
          ),
          Positioned(
            bottom: -120,
            left: -40,
            child: _GlowBubble(
              size: 300,
              color: const Color(0xFF7AE7FF).withValues(alpha: 0.18),
            ),
          ),
        ],
      ),
    );
  }
}

class _GlowBubble extends StatelessWidget {
  const _GlowBubble({required this.size, required this.color});

  final double size;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return Container(
      width: size,
      height: size,
      decoration: BoxDecoration(shape: BoxShape.circle, color: color),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 30, sigmaY: 30),
        child: const SizedBox.shrink(),
      ),
    );
  }
}
