import * as React from "react";

export function MediaImage(props: {
  src: string;
  alt: string;
  className?: string;
  fallbackLabel?: string; // big obvious text when it fails
}) {
  const { src, alt, className, fallbackLabel } = props;
  const [failed, setFailed] = React.useState(false);

  React.useEffect(() => {
    // reset when src changes
    setFailed(false);
  }, [src]);

  if (failed) {
    return (
      <div
        className={[
          "absolute inset-0 flex items-center justify-center",
          "bg-muted text-destructive font-semibold text-xs uppercase tracking-wider",
          className ?? "",
        ].join(" ")}
      >
        {fallbackLabel ?? "IMAGE FAILED"}
      </div>
    );
  }

  return (
    <img
      src={src}
      alt={alt}
      className={["absolute inset-0 h-full w-full object-cover", className ?? ""].join(" ")}
      loading="lazy"
      onError={() => setFailed(true)}
    />
  );
}
