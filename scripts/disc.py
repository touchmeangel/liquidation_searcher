import hashlib
import argparse

def anchor_discriminator(namespace: str, name: str) -> bytes:
  s = f"{namespace}:{name}".encode("utf-8")
  h = hashlib.sha256(s).digest()
  return h[:8]

def format_bytes(b: bytes) -> str:
  return "[" + ", ".join(f"0x{x:02x}" for x in b) + "]"

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("name", help="Instruction or account name")
    parser.add_argument("--ns", default="global", help="Namespace (default: global)")
    args = parser.parse_args()

    disc = anchor_discriminator(args.ns, args.name)
    print(f"Namespace: {args.ns}")
    print(f"Name:      {args.name}")
    print(f"Discriminator (raw bytes): {disc}")
    print(f"Rust literal: {format_bytes(disc)}")
