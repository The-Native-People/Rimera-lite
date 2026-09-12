'use client';

import { useEffect, useMemo, useRef, useState } from 'react';

type TokenType = 'command' | 'flag' | 'number' | 'operator' | 'path' | 'variable' | 'comment' | 'default';
type Phase = 'idle' | 'typing' | 'executing' | 'outputting' | 'pausing' | 'done';
type TerminalLine = { type: 'command' | 'output'; content: string };

interface TerminalSequenceProps {
  commands: string[];
  outputs?: Record<number, string[]>;
  username?: string;
  typingSpeed?: number;
  delayBetweenCommands?: number;
  initialDelay?: number;
}

const tokenColors: Record<TokenType, string> = {
  command: 'font-semibold text-[#ecf4f5]',
  flag: 'text-[#8adcf7]',
  number: 'text-[#d6c8ff]',
  operator: 'text-[#f4a48e]',
  path: 'text-[#9bdabf]',
  variable: 'text-[#d6c8ff]',
  comment: 'text-[#53646b]',
  default: 'text-[#b9c6ca]',
};

export function TerminalSequence({
  commands,
  outputs = {},
  username = 'rimera',
  typingSpeed = 24,
  delayBetweenCommands = 500,
  initialDelay = 350,
}: TerminalSequenceProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const [inView, setInView] = useState(false);
  const [lines, setLines] = useState<TerminalLine[]>([]);
  const [currentText, setCurrentText] = useState('');
  const [commandIndex, setCommandIndex] = useState(0);
  const [characterIndex, setCharacterIndex] = useState(0);
  const [outputIndex, setOutputIndex] = useState(-1);
  const [phase, setPhase] = useState<Phase>('idle');
  const [cursorVisible, setCursorVisible] = useState(true);

  const currentCommand = commands[commandIndex] ?? '';
  const currentOutputs = useMemo(() => outputs[commandIndex] ?? [], [commandIndex, outputs]);
  const isLastCommand = commandIndex === commands.length - 1;

  useEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const observer = new IntersectionObserver(([entry]) => {
      if (entry?.isIntersecting) {
        setInView(true);
        observer.disconnect();
      }
    }, { threshold: 0.1 });

    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!inView || phase !== 'idle') return;

    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      const completed = commands.flatMap((command, index) => [
        { type: 'command' as const, content: command },
        ...(outputs[index] ?? []).map((content) => ({ type: 'output' as const, content })),
      ]);
      setLines(completed);
      setPhase('done');
      return;
    }

    const timer = window.setTimeout(() => setPhase('typing'), initialDelay);
    return () => window.clearTimeout(timer);
  }, [commands, inView, initialDelay, outputs, phase]);

  useEffect(() => {
    if (phase !== 'typing') return;

    if (characterIndex < currentCommand.length) {
      const timer = window.setTimeout(() => {
        setCurrentText(currentCommand.slice(0, characterIndex + 1));
        setCharacterIndex((current) => current + 1);
      }, typingSpeed);
      return () => window.clearTimeout(timer);
    }

    const timer = window.setTimeout(() => setPhase('executing'), 90);
    return () => window.clearTimeout(timer);
  }, [characterIndex, currentCommand, phase, typingSpeed]);

  useEffect(() => {
    if (phase !== 'executing') return;

    setLines((current) => [...current, { type: 'command', content: currentCommand }]);
    setCurrentText('');
    if (currentOutputs.length > 0) {
      setOutputIndex(0);
      setPhase('outputting');
    } else {
      setPhase(isLastCommand ? 'done' : 'pausing');
    }
  }, [currentCommand, currentOutputs.length, isLastCommand, phase]);

  useEffect(() => {
    if (phase !== 'outputting') return;

    if (outputIndex >= 0 && outputIndex < currentOutputs.length) {
      const timer = window.setTimeout(() => {
        setLines((current) => [...current, { type: 'output', content: currentOutputs[outputIndex] ?? '' }]);
        setOutputIndex((current) => current + 1);
      }, 82);
      return () => window.clearTimeout(timer);
    }

    if (outputIndex >= currentOutputs.length) {
      const timer = window.setTimeout(() => setPhase(isLastCommand ? 'done' : 'pausing'), 260);
      return () => window.clearTimeout(timer);
    }
  }, [currentOutputs, isLastCommand, outputIndex, phase]);

  useEffect(() => {
    if (phase !== 'pausing') return;
    const timer = window.setTimeout(() => {
      setCharacterIndex(0);
      setOutputIndex(-1);
      setCommandIndex((current) => current + 1);
      setPhase('typing');
    }, delayBetweenCommands);
    return () => window.clearTimeout(timer);
  }, [delayBetweenCommands, phase]);

  useEffect(() => {
    const timer = window.setInterval(() => setCursorVisible((current) => !current), 530);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    const content = contentRef.current;
    if (content) content.scrollTop = content.scrollHeight;
  }, [lines, phase]);

  return (
    <div ref={containerRef} className="font-mono text-[clamp(0.62rem,1vw,0.74rem)]">
      <div ref={contentRef} className="h-64 overflow-y-auto px-4 py-4 [scrollbar-width:none] sm:h-72 sm:px-6 sm:py-5" aria-live="polite">
        {lines.map((line, index) => (
          <div key={`${line.content}-${index}`} className="whitespace-pre-wrap leading-6">
            {line.type === 'command' ? (
              <><Prompt username={username} /><SyntaxHighlightedText text={line.content} /></>
            ) : (
              <OutputLine line={line.content} />
            )}
          </div>
        ))}

        {phase === 'typing' && (
          <div className="whitespace-pre-wrap leading-6">
            <Prompt username={username} />
            <SyntaxHighlightedText text={currentText} />
            <span className="ml-0.5 inline-block h-4 w-1.5 translate-y-0.5 bg-[#dce8ea]" />
          </div>
        )}

        {(['done', 'pausing', 'outputting'] as Phase[]).includes(phase) && (
          <div className="whitespace-pre-wrap leading-6">
            <Prompt username={username} />
            <span className={`ml-0.5 inline-block h-4 w-1.5 translate-y-0.5 bg-[#dce8ea] transition-opacity ${cursorVisible ? 'opacity-100' : 'opacity-0'}`} />
          </div>
        )}
      </div>
    </div>
  );
}

function Prompt({ username }: { username: string }) {
  return (
    <span className="text-[#9aa6aa]">
      <span className="text-[#d9e2e4]">{username}</span>&nbsp;%&nbsp;
    </span>
  );
}

function SyntaxHighlightedText({ text }: { text: string }) {
  return tokenizeBash(text).map((token, index) => (
    <span key={`${token.value}-${index}`} className={tokenColors[token.type]}>{token.value}</span>
  ));
}

function tokenizeBash(text: string) {
  let isFirstWord = true;
  return text.split(/(\s+)/).map((value) => {
    let type: TokenType = 'default';
    if (!/^\s+$/.test(value)) {
      if (value.startsWith('#')) type = 'comment';
      else if (value.startsWith('$')) type = 'variable';
      else if (value.startsWith('-')) type = 'flag';
      else if (/^\d+$/.test(value)) type = 'number';
      else if (/^[|>&<]+$/.test(value)) type = 'operator';
      else if (value.includes('/') || value.endsWith('.py') || value.startsWith('.')) type = 'path';
      else if (isFirstWord) type = 'command';
      isFirstWord = type === 'operator';
    }
    return { type, value };
  });
}

function OutputLine({ line }: { line: string }) {
  const normalized = line.trimStart();
  const completed = normalized.startsWith('✓');
  const heading = normalized.startsWith('Rimera Lite');
  const running = normalized.startsWith('Running');
  const done = normalized.startsWith('Done');
  const metadata = normalized.startsWith('|-+');

  return (
    <span className={completed ? 'text-[#83bea3]' : heading ? 'font-semibold text-[#edf3f4]' : running ? 'text-[#dce8ea]' : done ? 'text-[#a7c6b7]' : metadata ? 'text-[#75858b]' : 'text-[#aeb9bd]'}>
      {line}
    </span>
  );
}
