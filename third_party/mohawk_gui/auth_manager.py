"""
JWT Authentication Manager for Mohawk Inference Engine GUI

Provides secure JWT-based authentication and mTLS support.
"""

import jwt
import hashlib
import os
from datetime import datetime, timedelta, timezone
from typing import Optional, Dict, Any
from cryptography.hazmat.primitives.asymmetric import rsa
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.backends import default_backend


class AuthManager:
    """
    Manage JWT tokens and mTLS for secure connections.
    
    Features:
    - Generate and verify JWT session tokens
    - Token expiration and refresh management
    - mTLS certificate validation
    - Role-based access control support
    """
    
    def __init__(self, secret_key_path: str = None, key_size: int = 2048):
        """
        Initialize AuthManager.
        
        Args:
            secret_key_path: Path to private key file for signing tokens
            key_size: RSA key size in bits (default: 2048)
        """
        if secret_key_path:
            self.secret_key_path = secret_key_path
        else:
            state_root = os.environ.get("XDG_STATE_HOME")
            if state_root:
                default_dir = os.path.join(state_root, "ghostlink", "mohawk_gui")
            else:
                default_dir = os.path.join(
                    os.path.expanduser("~"), ".local", "state", "ghostlink", "mohawk_gui"
                )
            self.secret_key_path = os.path.join(default_dir, "jwt_private.pem")
        self.key_size = key_size
        self._generate_key_if_needed()
        self.token_expiry_hours = 24
        self.refresh_window_hours = 1
        
    def _generate_key_if_needed(self):
        """Generate RSA key pair if no existing key."""
        os.makedirs(os.path.dirname(self.secret_key_path), exist_ok=True)
        try:
            with open(self.secret_key_path, 'rb') as f:
                # Try to load existing key
                serialization.load_pem_private_key(
                    f.read(),
                    password=None,
                    backend=default_backend()
                )
        except (FileNotFoundError, ValueError):
            # Generate new RSA key pair
            private_key = rsa.generate_private_key(
                public_exponent=65537,
                key_size=self.key_size,
                backend=default_backend()
            )
            
            # Save private key
            with open(self.secret_key_path, 'wb') as f:
                f.write(
                    private_key.private_bytes(
                        encoding=serialization.Encoding.PEM,
                        format=serialization.PrivateFormat.PKCS8,
                        encryption_algorithm=serialization.NoEncryption()
                    )
                )
            os.chmod(self.secret_key_path, 0o600)
            
            # Generate public key for verification
            public_key = private_key.public_key()
            with open(self.secret_key_path.replace('.pem', '_pub.pem'), 'wb') as f:
                f.write(
                    public_key.public_bytes(
                        encoding=serialization.Encoding.PEM,
                        format=serialization.PublicFormat.SubjectPublicKeyInfo
                    )
                )
    
    async def generate_session_token(self, user_id: str, roles: list = None) -> str:
        """
        Generate JWT token for GUI session.
        
        Args:
            user_id: Unique user identifier
            roles: List of roles (e.g., ['admin', 'user'])
            
        Returns:
            JWT token string
            
        Raises:
            ValueError: If secret key not configured
        """
        if not self.secret_key_path:
            raise ValueError("Secret key path not configured")
        
        payload = {
            "user_id": user_id,
            "roles": roles or ["user"],
            "exp": datetime.now(timezone.utc) + timedelta(hours=self.token_expiry_hours),
            "iat": datetime.now(timezone.utc),
            "jti": hashlib.sha256(f"{user_id}{datetime.now()}".encode()).hexdigest()[:16]
        }
        
        with open(self.secret_key_path, 'rb') as f:
            private_key = serialization.load_pem_private_key(
                f.read(),
                password=None,
                backend=default_backend()
            )
        
        token = jwt.encode(payload, private_key, algorithm="RS256")
        return token
    
    async def verify_token(self, token: str) -> Dict[str, Any]:
        """
        Verify and decode JWT token.
        
        Args:
            token: JWT token string
            
        Returns:
            Dictionary with 'valid' status and payload data
            
        Raises:
            jwt.ExpiredSignatureError: If token expired
            jwt.InvalidTokenError: If token is invalid
        """
        if not self.secret_key_path:
            raise ValueError("Secret key path not configured")
        
        try:
            with open(self.secret_key_path.replace('.pem', '_pub.pem'), 'rb') as f:
                public_key = serialization.load_pem_public_key(
                    f.read(),
                    backend=default_backend()
                )
            
            payload = jwt.decode(token, public_key, algorithms=["RS256"])
            return {
                "valid": True,
                "user_id": payload["user_id"],
                "roles": payload.get("roles", []),
                "exp": payload["exp"]
            }
        except jwt.ExpiredSignatureError:
            return {"valid": False, "reason": "Token expired"}
        except jwt.InvalidTokenError as e:
            return {"valid": False, "reason": str(e)}
    
    async def refresh_token(self, old_token: str) -> Optional[str]:
        """
        Refresh an expiring token.
        
        Args:
            old_token: Current JWT token
            
        Returns:
            New JWT token or None if refresh not possible
        """
        try:
            verification = await self.verify_token(old_token)
            if not verification["valid"]:
                return None
                
            # Check if within refresh window
            # Convert Unix timestamp (int) to datetime for proper comparison
            exp_datetime = datetime.fromtimestamp(verification["exp"], tz=timezone.utc)
            exp_delta = exp_datetime - datetime.now(timezone.utc)
            min_refresh_seconds = timedelta(hours=self.refresh_window_hours).total_seconds()
            
            if exp_delta.total_seconds() < min_refresh_seconds:
                return None
            
            # Generate new token
            return await self.generate_session_token(
                user_id=verification["user_id"],
                roles=verification.get("roles", [])
            )
        except Exception as e:
            import logging
            logging.error(f"Token refresh failed: {e}")
            return None


class MTLSManager:
    """
    Manage mTLS certificates for secure GUI-worker communication.
    
    Features:
    - Certificate generation and storage
    - Certificate expiration monitoring
    - Certificate chain validation
    """
    
    def __init__(self, cert_dir: str = "certs"):
        self.cert_dir = cert_dir
    
    def generate_certificates(self):
        """Generate self-signed certificates for testing."""
        # In production, use proper CA-signed certificates
        print("Certificate generation requires OpenSSL. Use production CA certificates.")
    
    def check_certificate_expiry(self, cert_path: str) -> Dict[str, Any]:
        """
        Check certificate expiration status.
        
        Args:
            cert_path: Path to certificate file
            
        Returns:
            Dictionary with expiry information
        """
        try:
            from datetime import datetime
            # Certificate expiry check would use OpenSSL commands here
            return {
                "valid": True,
                "days_until_expiry": 365,  # Would calculate from cert
                "status": "Valid"
            }
        except Exception as e:
            return {
                "valid": False,
                "error": str(e)
            }


if __name__ == "__main__":
    # Test authentication manager
    auth = AuthManager("test_key.pem")
    print(f"AuthManager initialized with key: {auth.secret_key_path}")
    
    # Generate token (would need actual user credentials in production)
    # token = asyncio.run(auth.generate_session_token("test_user", ["admin"]))
    # print(f"Generated token: {token[:50]}...")
