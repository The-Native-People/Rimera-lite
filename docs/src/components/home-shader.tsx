'use client';

import { Dithering } from '@paper-design/shaders-react';
import { useTheme } from 'fumadocs-ui/provider/base';
import { useEffect, useState } from 'react';

function useMotionPreference() {
  const [canAnimate, setCanAnimate] = useState(false);

  useEffect(() => {
    const motion = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => setCanAnimate(!motion.matches);

    update();
    motion.addEventListener('change', update);
    return () => motion.removeEventListener('change', update);
  }, []);

  return canAnimate;
}

export function HomeShader() {
  const canAnimate = useMotionPreference();
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === 'dark';

  return (
    <Dithering
      className="rimera-paper-shader"
      colorBack={isDark ? '#05080d' : '#edf3f4'}
      colorFront={isDark ? '#2d819e' : '#77bdd3'}
      shape="warp"
      type="4x4"
      size={2.4}
      scale={1.18}
      rotation={-7}
      offsetX={0.16}
      offsetY={-0.05}
      speed={canAnimate ? 0.045 : 0}
      minPixelRatio={1}
      maxPixelCount={2_073_600}
      aria-hidden="true"
    />
  );
}
