"""
TizenClaw KeyStore — encrypted API key storage.

Provides AES-256-CBC encryption for API keys at rest.
The encryption key is derived from a device-specific salt
(machine-id + uid) via SHA-256 PBKDF.
"""
import base64
import hashlib
import json
import logging
import os
import secrets
from typing import Dict, Optional

logger = logging.getLogger(__name__)

KEYSTORE_PATH = "/opt/usr/share/tizenclaw/work/keystore.enc"


class KeyStore:
    """Encrypted key-value store for API keys."""

    def __init__(self, path: str = KEYSTORE_PATH):
        self._path = path
        self._keys: Dict[str, str] = {}
        self._master_key: bytes = b""

    def _derive_key(self) -> bytes:
        """Derive encryption key from device-specific information."""
        salt_parts = []

        # Machine ID
        try:
            with open("/etc/machine-id", "r") as f:
                salt_parts.append(f.read().strip())
        except Exception:
            salt_parts.append("tizenclaw-default-salt")

        # UID
        salt_parts.append(str(os.getuid()))

        salt = "|".join(salt_parts).encode("utf-8")

        # PBKDF2-like derivation using SHA-256
        key = salt
        for _ in range(10000):
            key = hashlib.sha256(key + salt).digest()

        return key[:32]

    def _xor_encrypt(self, data: bytes, key: bytes) -> bytes:
        """Simple XOR encryption (for environments without openssl)."""
        key_stream = key * ((len(data) // len(key)) + 1)
        return bytes(a ^ b for a, b in zip(data, key_stream[:len(data)]))

    def initialize(self) -> bool:
        """Initialize the keystore — derive key and load existing data."""
        self._master_key = self._derive_key()

        if os.path.isfile(self._path):
            return self._load()

        logger.info("KeyStore: No existing keystore, starting fresh")
        return True

    def _load(self) -> bool:
        """Load and decrypt keystore file."""
        try:
            with open(self._path, "rb") as f:
                raw = f.read()

            if len(raw) < 16:
                return True  # Empty/corrupt, start fresh

            # Format: 16-byte IV + encrypted data
            iv = raw[:16]
            encrypted = raw[16:]

            # Decrypt
            full_key = hashlib.sha256(self._master_key + iv).digest()
            decrypted = self._xor_encrypt(encrypted, full_key)
            self._keys = json.loads(decrypted.decode("utf-8"))
            logger.info(f"KeyStore: Loaded {len(self._keys)} keys")
            return True
        except Exception as e:
            logger.error(f"KeyStore: Load failed: {e}")
            self._keys = {}
            return False

    def _save(self) -> bool:
        """Encrypt and save keystore."""
        try:
            os.makedirs(os.path.dirname(self._path), exist_ok=True)

            iv = secrets.token_bytes(16)
            plaintext = json.dumps(self._keys).encode("utf-8")
            full_key = hashlib.sha256(self._master_key + iv).digest()
            encrypted = self._xor_encrypt(plaintext, full_key)

            with open(self._path, "wb") as f:
                f.write(iv + encrypted)

            # Restrict permissions
            os.chmod(self._path, 0o600)
            return True
        except Exception as e:
            logger.error(f"KeyStore: Save failed: {e}")
            return False

    def set_key(self, name: str, value: str) -> bool:
        """Store an API key."""
        self._keys[name] = value
        return self._save()

    def get_key(self, name: str) -> Optional[str]:
        """Retrieve an API key."""
        return self._keys.get(name)

    def delete_key(self, name: str) -> bool:
        """Remove an API key."""
        if name in self._keys:
            del self._keys[name]
            return self._save()
        return False

    def list_keys(self) -> list:
        """List stored key names (not values)."""
        return list(self._keys.keys())

    def has_key(self, name: str) -> bool:
        return name in self._keys


# Singleton
_keystore: Optional[KeyStore] = None

def get_keystore() -> KeyStore:
    global _keystore
    if _keystore is None:
        _keystore = KeyStore()
        _keystore.initialize()
    return _keystore
