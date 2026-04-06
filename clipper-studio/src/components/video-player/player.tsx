import { useRef, useEffect, useMemo, useState } from "react";
import { MediaPlayer, MediaProvider } from "@vidstack/react";
import {
  DefaultVideoLayout,
  defaultLayoutIcons,
} from "@vidstack/react/player/layouts/default";
import mpegts from "mpegts.js";
import { getAppInfo } from "@/services/workspace";
import "@vidstack/react/player/styles/default/theme.css";
import "@vidstack/react/player/styles/default/layouts/video.css";

interface VideoPlayerProps {
  src: string;
  title?: string;
  /** Callback when the underlying <video> element is mounted */
  onVideoRef?: (el: HTMLVideoElement | null) => void;
}

function getExtension(filePath: string): string {
  const name = filePath.split(/[/\\]/).pop() ?? "";
  return name.split(".").pop()?.toLowerCase() ?? "";
}

function needsMpegts(ext: string): boolean {
  return ["flv", "ts"].includes(ext);
}

function getMimeType(ext: string): string {
  const map: Record<string, string> = {
    mp4: "video/mp4",
    mkv: "video/x-matroska",
    webm: "video/webm",
    mov: "video/quicktime",
  };
  return map[ext] ?? "video/mp4";
}

/**
 * Build media server URL for a local file.
 * The Rust-side media server provides proper HTTP Range support.
 */
function buildMediaUrl(port: number, filePath: string): string {
  return `http://127.0.0.1:${port}/serve?path=${encodeURIComponent(filePath)}`;
}

/**
 * Hook to get the media server port from backend.
 */
function useMediaServerPort(): number | null {
  const [port, setPort] = useState<number | null>(null);
  useEffect(() => {
    getAppInfo()
      .then((info) => setPort(info.media_server_port))
      .catch(console.error);
  }, []);
  return port;
}

/**
 * FLV/TS player using mpegts.js + native <video>.
 */
function MpegtsPlayer({
  src,
  title,
  mediaUrl,
  onVideoRef,
}: VideoPlayerProps & { mediaUrl: string }) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const playerRef = useRef<mpegts.Player | null>(null);

  // Notify parent of video element mount/unmount
  useEffect(() => {
    onVideoRef?.(videoRef.current ?? null);
    return () => onVideoRef?.(null);
  }, [onVideoRef]);

  useEffect(() => {
    const videoEl = videoRef.current;
    if (!videoEl) return;

    if (!mpegts.isSupported()) {
      console.error("[MpegtsPlayer] MSE is not supported");
      return;
    }

    console.log("[MpegtsPlayer] Loading:", mediaUrl);

    const player = mpegts.createPlayer(
      {
        type: getExtension(src) === "ts" ? "mpegts" : "flv",
        url: mediaUrl,
      },
      {
        enableWorker: false,
        lazyLoadMaxDuration: 120,
        seekType: "range",
      }
    );

    player.on(mpegts.Events.ERROR, (errorType, errorDetail, errorInfo) => {
      console.warn("[MpegtsPlayer] Error:", errorType, errorDetail, errorInfo);
    });

    player.attachMediaElement(videoEl);
    player.load();
    playerRef.current = player;

    return () => {
      player.pause();
      player.unload();
      player.detachMediaElement();
      player.destroy();
      playerRef.current = null;
    };
  }, [src, mediaUrl]);

  return (
    <div className="relative w-full aspect-video bg-black rounded-lg overflow-hidden">
      <video
        ref={videoRef}
        title={title}
        controls
        className="w-full h-full"
      />
    </div>
  );
}

/**
 * MP4/WebM/MKV player using vidstack.
 */
function VidstackPlayer({
  title,
  mediaUrl,
  ext,
  onVideoRef,
}: {
  title?: string;
  mediaUrl: string;
  ext: string;
  onVideoRef?: (el: HTMLVideoElement | null) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mimeType = getMimeType(ext);
  const mediaSrc = useMemo(
    () => ({ src: mediaUrl, type: mimeType }),
    [mediaUrl, mimeType]
  );

  // Find and forward the underlying <video> element
  useEffect(() => {
    const el = containerRef.current?.querySelector("video") ?? null;
    onVideoRef?.(el);
    return () => onVideoRef?.(null);
  }, [onVideoRef]);

  return (
    <div ref={containerRef}>
      <MediaPlayer
        title={title}
        src={mediaSrc as any}
        className="w-full aspect-video bg-black rounded-lg overflow-hidden"
      >
        <MediaProvider />
        <DefaultVideoLayout icons={defaultLayoutIcons} />
      </MediaPlayer>
    </div>
  );
}

/**
 * Video player: routes to appropriate backend via local media server.
 * - All formats served through Rust HTTP server (proper Range support)
 * - FLV/TS → mpegts.js demuxer
 * - MP4/WebM/MKV → vidstack
 */
export function VideoPlayer({ src, title, onVideoRef }: VideoPlayerProps) {
  const port = useMediaServerPort();
  const ext = getExtension(src);

  if (port === null) {
    return (
      <div className="w-full aspect-video bg-black rounded-lg flex items-center justify-center text-white/50">
        加载播放器...
      </div>
    );
  }

  const mediaUrl = buildMediaUrl(port, src);

  if (needsMpegts(ext)) {
    return (
      <MpegtsPlayer
        src={src}
        title={title}
        mediaUrl={mediaUrl}
        onVideoRef={onVideoRef}
      />
    );
  }

  return (
    <VidstackPlayer
      title={title}
      mediaUrl={mediaUrl}
      ext={ext}
      onVideoRef={onVideoRef}
    />
  );
}
