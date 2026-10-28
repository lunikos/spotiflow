(() => {
  const scrollLinked = () =>
    document.getAnimations().filter((a) => a.timeline && !(a.timeline instanceof DocumentTimeline));
  let timer = 0;
  const rest = () => scrollLinked().forEach((a) => a.pause());
  const wake = () => {
    if (!timer) scrollLinked().forEach((a) => a.play());
    clearTimeout(timer);
    timer = setTimeout(() => ((timer = 0), rest()), 300);
  };
  for (const e of ["wheel", "scroll", "touchmove", "keydown", "pointerdown"])
    addEventListener(e, wake, { capture: true, passive: true });
  setInterval(() => timer || rest(), 2000);
})();
