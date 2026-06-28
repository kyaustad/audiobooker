"use client";

import { LogOut, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

type Download = {
  id: number;
  name: string | null;
  magnetUri: string;
  status: string;
  progress: number;
  downloadSpeed: number;
  eta: number;
  destinationPath: string | null;
  errorMessage: string | null;
  createdAt: string;
  copiedAt: string | null;
};

function formatBytes(bytes: number) {
  if (bytes <= 0) return "0 B/s";
  const units = ["B/s", "KB/s", "MB/s", "GB/s"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(1)} ${units[index]}`;
}

function formatEta(seconds: number) {
  if (seconds <= 0 || !Number.isFinite(seconds)) return "—";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

function statusVariant(status: string) {
  switch (status) {
    case "copied":
      return "default" as const;
    case "completed":
    case "copying":
      return "secondary" as const;
    case "error":
      return "destructive" as const;
    default:
      return "outline" as const;
  }
}

export function Dashboard({ username }: { username: string }) {
  const [input, setInput] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);

  const refreshDownloads = useCallback(async () => {
    const response = await fetch("/api/downloads/sync");
    if (!response.ok) {
      throw new Error("Failed to refresh downloads");
    }
    const data = await response.json();
    setDownloads(data.downloads);
  }, []);

  useEffect(() => {
    refreshDownloads()
      .catch((error) => {
        toast.error(error instanceof Error ? error.message : "Failed to load downloads");
      })
      .finally(() => setLoading(false));

    const interval = setInterval(() => {
      refreshDownloads().catch(() => undefined);
    }, 10000);

    return () => clearInterval(interval);
  }, [refreshDownloads]);

  async function handleAddMagnet(event: React.FormEvent) {
    event.preventDefault();
    setSubmitting(true);

    try {
      const response = await fetch("/api/downloads", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          input,
          name: displayName || undefined,
        }),
      });
      const data = await response.json();

      if (!response.ok) {
        throw new Error(data.error ?? "Failed to add magnet");
      }

      setInput("");
      setDisplayName("");
      toast.success("Download added to qBittorrent");
      await refreshDownloads();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Failed to add magnet");
    } finally {
      setSubmitting(false);
    }
  }

  async function handleDelete(downloadId: number) {
    try {
      const response = await fetch(`/api/downloads/${downloadId}`, {
        method: "DELETE",
      });
      if (!response.ok) {
        const data = await response.json();
        throw new Error(data.error ?? "Failed to remove download");
      }
      toast.success("Download removed");
      await refreshDownloads();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : "Failed to remove download");
    }
  }

  async function handleLogout() {
    await fetch("/api/auth/logout", { method: "POST" });
    window.location.href = "/login";
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 p-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Audiobooker</h1>
          <p className="text-sm text-muted-foreground">
            Signed in as {username}. Add magnets and track downloads to your library.
          </p>
        </div>
        <Button variant="outline" onClick={handleLogout}>
          <LogOut />
          Sign out
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Add download</CardTitle>
          <CardDescription>
            Paste a magnet link or a 40-character info hash (hex). Raw hashes are converted
            to magnet links automatically. Completed downloads are copied to your audiobook
            library folder.
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form className="flex flex-col gap-4" onSubmit={handleAddMagnet}>
            <div className="grid gap-4 md:grid-cols-2">
              <div className="space-y-2 md:col-span-2">
                <Label htmlFor="input">Magnet link or info hash</Label>
                <Input
                  id="input"
                  placeholder="magnet:?xt=urn:btih:... or a1b2c3d4e5f6..."
                  value={input}
                  onChange={(event) => setInput(event.target.value)}
                  required
                />
              </div>
              <div className="space-y-2 md:col-span-2">
                <Label htmlFor="display-name">Display name (optional)</Label>
                <Input
                  id="display-name"
                  placeholder="Audiobook title"
                  value={displayName}
                  onChange={(event) => setDisplayName(event.target.value)}
                />
              </div>
            </div>
            <div>
              <Button type="submit" disabled={submitting}>
                {submitting ? "Adding..." : "Add download"}
              </Button>
            </div>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Your downloads</CardTitle>
          <CardDescription>
            Progress syncs automatically from qBittorrent every few seconds.
          </CardDescription>
        </CardHeader>
        <CardContent>
          {loading ? (
            <p className="text-sm text-muted-foreground">Loading downloads...</p>
          ) : downloads.length === 0 ? (
            <p className="text-sm text-muted-foreground">No downloads yet.</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Name</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead>Progress</TableHead>
                  <TableHead>Speed</TableHead>
                  <TableHead>ETA</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {downloads.map((download) => (
                  <TableRow key={download.id}>
                    <TableCell className="max-w-xs">
                      <div className="space-y-1">
                        <p className="truncate font-medium">
                          {download.name ?? "Fetching name..."}
                        </p>
                        {download.destinationPath ? (
                          <p className="truncate text-xs text-muted-foreground">
                            Copied to {download.destinationPath}
                          </p>
                        ) : null}
                        {download.errorMessage ? (
                          <p className="text-xs text-destructive">{download.errorMessage}</p>
                        ) : null}
                      </div>
                    </TableCell>
                    <TableCell>
                      <Badge variant={statusVariant(download.status)}>
                        {download.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="min-w-40">
                      <div className="space-y-2">
                        <Progress value={download.progress * 100} />
                        <p className="text-xs text-muted-foreground">
                          {Math.round(download.progress * 100)}%
                        </p>
                      </div>
                    </TableCell>
                    <TableCell>{formatBytes(download.downloadSpeed)}</TableCell>
                    <TableCell>{formatEta(download.eta)}</TableCell>
                    <TableCell className="text-right">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => handleDelete(download.id)}
                        aria-label="Remove download"
                      >
                        <Trash2 />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
