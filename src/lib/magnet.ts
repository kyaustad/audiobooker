const BASE32 = "abcdefghijklmnopqrstuvwxyz234567";

function base32ToHex(base32: string): string {
  let bits = "";
  for (const char of base32.toLowerCase()) {
    const value = BASE32.indexOf(char);
    if (value === -1) continue;
    bits += value.toString(2).padStart(5, "0");
  }

  let hex = "";
  for (let i = 0; i + 4 <= bits.length; i += 4) {
    hex += parseInt(bits.slice(i, i + 4), 2).toString(16);
  }
  return hex;
}

export function parseMagnetHash(magnetUri: string): string | null {
  const match = magnetUri.match(/xt=urn:btih:([a-fA-F0-9]{40}|[a-zA-Z2-7]{32})/i);
  if (!match) {
    return null;
  }

  const hash = match[1];
  if (hash.length === 40) {
    return hash.toLowerCase();
  }
  return base32ToHex(hash);
}

export function parseMagnetName(magnetUri: string): string | null {
  const match = magnetUri.match(/dn=([^&]+)/i);
  if (!match) {
    return null;
  }
  try {
    return decodeURIComponent(match[1].replace(/\+/g, " "));
  } catch {
    return match[1];
  }
}

export function isValidMagnetUri(magnetUri: string) {
  return magnetUri.trim().startsWith("magnet:?") && parseMagnetHash(magnetUri) !== null;
}

export function normalizeInfoHash(input: string): string | null {
  const compact = input.trim().replace(/[\s-]/g, "");

  if (/^[a-fA-F0-9]{40}$/.test(compact)) {
    return compact.toLowerCase();
  }

  if (/^[a-zA-Z2-7]{32}$/i.test(compact)) {
    return base32ToHex(compact);
  }

  return null;
}

export function buildMagnetUri(infoHashHex: string, name?: string | null) {
  const params = new URLSearchParams();
  params.set("xt", `urn:btih:${infoHashHex}`);
  if (name?.trim()) {
    params.set("dn", name.trim());
  }
  return `magnet:?${params.toString()}`;
}

export function parseDownloadInput(input: string, name?: string | null) {
  const trimmed = input.trim();
  if (!trimmed) {
    return null;
  }

  if (trimmed.startsWith("magnet:?")) {
    const infoHash = parseMagnetHash(trimmed);
    if (!infoHash) {
      return null;
    }

    return {
      magnetUri: trimmed,
      infoHash,
      name: parseMagnetName(trimmed) ?? name?.trim() ?? null,
    };
  }

  const infoHash = normalizeInfoHash(trimmed);
  if (!infoHash) {
    return null;
  }

  const displayName = name?.trim() || null;
  return {
    magnetUri: buildMagnetUri(infoHash, displayName),
    infoHash,
    name: displayName,
  };
}

export function isValidDownloadInput(input: string) {
  return parseDownloadInput(input) !== null;
}
