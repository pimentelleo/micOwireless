import 'dart:convert';
import 'dart:typed_data';

import 'package:cryptography/cryptography.dart';

const protocolName = 'mow2';
const discoveryRequestKind = 'discover';
const discoveryResponseKind = 'discover_response';
const discoveryPortOffset = 1;

const _protocolMagic = [0x4D, 0x4F, 0x57, 0x32]; // "MOW2"
const _protocolVersion = 2;
const _protocolFlagEncrypted = 0x01;
const _payloadFormatPcm16 = 1;
const _headerLength = 28;

final _streamCipher = Chacha20.poly1305Aead();
final _keyDeriveHash = Sha256();

Future<SecretKey> derivePairingKey(String pairCode) async {
  final normalized = pairCode.trim();
  if (normalized.length < 6) {
    throw ArgumentError('Pair code must contain at least 6 characters.');
  }
  final hash = await _keyDeriveHash.hash(utf8.encode(normalized));
  return SecretKey(hash.bytes);
}

Future<Uint8List> buildAudioPacket({
  required int sequence,
  required int sessionId,
  required int sampleRate,
  required int channels,
  required Uint8List pcm16Payload,
  required bool secureMode,
  SecretKey? pairingKey,
}) async {
  if (secureMode && pairingKey == null) {
    throw ArgumentError('Secure mode requires a pairing key.');
  }

  final payload = secureMode
      ? await _encryptPayload(
          pcm16Payload: pcm16Payload,
          sequence: sequence,
          sessionId: sessionId,
          pairingKey: pairingKey!,
          sampleRate: sampleRate,
          channels: channels,
        )
      : pcm16Payload;

  if (payload.length > 0xFFFF) {
    throw ArgumentError('Payload too large for protocol frame.');
  }

  final header = Uint8List(_headerLength);
  header.setAll(0, _protocolMagic);
  final headerData = ByteData.sublistView(header);
  headerData.setUint8(4, _protocolVersion);
  headerData.setUint8(5, secureMode ? _protocolFlagEncrypted : 0);
  headerData.setUint16(6, payload.length, Endian.little);
  headerData.setUint32(8, sequence, Endian.little);
  headerData.setUint64(12, sessionId, Endian.little);
  headerData.setUint32(20, sampleRate, Endian.little);
  headerData.setUint16(24, channels, Endian.little);
  headerData.setUint16(26, _payloadFormatPcm16, Endian.little);

  final packet = Uint8List(_headerLength + payload.length);
  packet.setRange(0, _headerLength, header);
  packet.setRange(_headerLength, packet.length, payload);
  return packet;
}

Future<Uint8List> _encryptPayload({
  required Uint8List pcm16Payload,
  required int sequence,
  required int sessionId,
  required SecretKey pairingKey,
  required int sampleRate,
  required int channels,
}) async {
  final aad = Uint8List(_headerLength);
  aad.setAll(0, _protocolMagic);
  final aadData = ByteData.sublistView(aad);
  final encryptedPayloadLength = pcm16Payload.length + 16;
  aadData.setUint8(4, _protocolVersion);
  aadData.setUint8(5, _protocolFlagEncrypted);
  aadData.setUint16(6, encryptedPayloadLength, Endian.little);
  aadData.setUint32(8, sequence, Endian.little);
  aadData.setUint64(12, sessionId, Endian.little);
  aadData.setUint32(20, sampleRate, Endian.little);
  aadData.setUint16(24, channels, Endian.little);
  aadData.setUint16(26, _payloadFormatPcm16, Endian.little);

  final nonce = _buildNonce(sessionId: sessionId, sequence: sequence);
  final box = await _streamCipher.encrypt(
    pcm16Payload,
    secretKey: pairingKey,
    nonce: nonce,
    aad: aad,
  );
  return Uint8List.fromList([...box.cipherText, ...box.mac.bytes]);
}

Uint8List _buildNonce({required int sessionId, required int sequence}) {
  final nonce = Uint8List(12);
  final data = ByteData.sublistView(nonce);
  data.setUint64(0, sessionId, Endian.little);
  data.setUint32(8, sequence, Endian.little);
  return nonce;
}
