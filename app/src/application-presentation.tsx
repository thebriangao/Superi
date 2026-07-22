import {
  cloneElement,
  createContext,
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

import {
  type ApplicationFailurePresentation,
  type ApplicationNotification,
  type ApplicationNotificationInput,
  type ApplicationNotificationState,
  type ApplicationOperationalStatus,
  type ApplicationProgressPresentation,
  createApplicationNotificationState,
  placeApplicationContextMenu,
  reduceApplicationNotificationState,
} from "./application-presentation.ts";

export interface ApplicationContextMenuItem {
  readonly id: string;
  readonly label: string;
  readonly detail?: string;
  readonly shortcut?: string;
  readonly disabled?: boolean;
  readonly tone?: "default" | "danger";
  readonly onSelect: () => void;
}

export interface ApplicationContextMenuRequest {
  readonly label: string;
  readonly x: number;
  readonly y: number;
  readonly items: readonly ApplicationContextMenuItem[];
  readonly returnFocus?: HTMLElement | null;
}

interface OpenApplicationContextMenu extends ApplicationContextMenuRequest {
  readonly left: number;
  readonly top: number;
}

interface ApplicationPresentationContextValue {
  readonly notificationState: ApplicationNotificationState;
  readonly publishNotification: (
    notification: ApplicationNotificationInput,
  ) => void;
  readonly dismissNotification: (id: string) => void;
  readonly clearNotifications: () => void;
  readonly openContextMenu: (request: ApplicationContextMenuRequest) => void;
  readonly closeContextMenu: () => void;
}

const ApplicationPresentationContext =
  createContext<ApplicationPresentationContextValue | null>(null);

export function useApplicationPresentation(): ApplicationPresentationContextValue {
  const value = useContext(ApplicationPresentationContext);
  if (value === null) {
    throw new Error(
      "Application presentation controls require ApplicationPresentationProvider.",
    );
  }
  return value;
}

export function ApplicationPresentationProvider({
  children,
}: {
  readonly children: ReactNode;
}) {
  const [notificationState, dispatchNotification] = useReducer(
    reduceApplicationNotificationState,
    undefined,
    createApplicationNotificationState,
  );
  const [contextMenu, setContextMenu] =
    useState<OpenApplicationContextMenu | null>(null);
  const contextMenuReturnFocus = useRef<HTMLElement | null>(null);

  const publishNotification = useCallback(
    (notification: ApplicationNotificationInput) => {
      dispatchNotification({ type: "publish", notification });
    },
    [],
  );
  const dismissNotification = useCallback((id: string) => {
    dispatchNotification({ type: "dismiss", id });
  }, []);
  const clearNotifications = useCallback(() => {
    dispatchNotification({ type: "clear" });
  }, []);
  const closeContextMenu = useCallback(() => {
    setContextMenu(null);
    const returnFocus = contextMenuReturnFocus.current;
    contextMenuReturnFocus.current = null;
    if (returnFocus?.isConnected) {
      window.requestAnimationFrame(() => {
        if (returnFocus.isConnected) returnFocus.focus();
      });
    }
  }, []);
  const openContextMenu = useCallback(
    (request: ApplicationContextMenuRequest) => {
      const menuWidth = 272;
      const menuHeight = Math.max(56, request.items.length * 42 + 32);
      const position = placeApplicationContextMenu({
        x: request.x,
        y: request.y,
        menuWidth,
        menuHeight,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
      });
      contextMenuReturnFocus.current = request.returnFocus ?? null;
      setContextMenu({ ...request, ...position });
    },
    [],
  );

  useEffect(() => {
    if (contextMenu === null) {
      return undefined;
    }
    const close = () => setContextMenu(null);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
    };
  }, [contextMenu]);

  const value = useMemo<ApplicationPresentationContextValue>(
    () => ({
      notificationState,
      publishNotification,
      dismissNotification,
      clearNotifications,
      openContextMenu,
      closeContextMenu,
    }),
    [
      clearNotifications,
      closeContextMenu,
      dismissNotification,
      notificationState,
      openContextMenu,
      publishNotification,
    ],
  );

  return (
    <ApplicationPresentationContext.Provider value={value}>
      {children}
      <ApplicationToastRegion />
      {contextMenu === null
        ? null
        : createPortal(
            <ApplicationContextMenu
              menu={contextMenu}
              onClose={closeContextMenu}
            />,
            document.body,
          )}
    </ApplicationPresentationContext.Provider>
  );
}

function ApplicationContextMenu({
  menu,
  onClose,
}: {
  readonly menu: OpenApplicationContextMenu;
  readonly onClose: () => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({
    left: menu.left,
    top: menu.top,
  });

  useLayoutEffect(() => {
    const rectangle = menuRef.current?.getBoundingClientRect();
    if (rectangle === undefined) return;
    const next = placeApplicationContextMenu({
      x: menu.x,
      y: menu.y,
      menuWidth: rectangle.width,
      menuHeight: rectangle.height,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
    });
    setPosition(next);
  }, [menu]);

  useEffect(() => {
    const firstItem = menuRef.current?.querySelector<HTMLButtonElement>(
      '[role="menuitem"]:not(:disabled)',
    );
    firstItem?.focus();
  }, [menu]);

  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const items = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>(
        '[role="menuitem"]:not(:disabled)',
      ),
    );
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (items.length === 0) {
      return;
    }
    const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
    let nextIndex: number | null = null;
    if (event.key === "ArrowDown") {
      nextIndex = (currentIndex + 1) % items.length;
    } else if (event.key === "ArrowUp") {
      nextIndex = (currentIndex - 1 + items.length) % items.length;
    } else if (event.key === "Home") {
      nextIndex = 0;
    } else if (event.key === "End") {
      nextIndex = items.length - 1;
    }
    if (nextIndex !== null) {
      event.preventDefault();
      items[nextIndex]?.focus();
    }
  };

  return (
    <div
      className="application-context-menu-shield"
      onContextMenu={(event) => event.preventDefault()}
      onMouseDown={onClose}
    >
      <div
        aria-label={menu.label}
        className="application-context-menu"
        onKeyDown={handleKeyDown}
        onMouseDown={(event) => event.stopPropagation()}
        ref={menuRef}
        role="menu"
        style={position}
      >
        <p className="application-context-menu__label">{menu.label}</p>
        {menu.items.map((item) => (
          <button
            className="application-context-menu__item"
            data-tone={item.tone ?? "default"}
            disabled={item.disabled}
            key={item.id}
            onClick={() => {
              item.onSelect();
              onClose();
            }}
            role="menuitem"
            type="button"
          >
            <span>
              <strong>{item.label}</strong>
              {item.detail === undefined ? null : <small>{item.detail}</small>}
            </span>
            {item.shortcut === undefined ? null : (
              <kbd>{item.shortcut}</kbd>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

export function ApplicationTooltip({
  children,
  content,
  placement = "top",
}: {
  readonly children: ReactElement<{ "aria-describedby"?: string }>;
  readonly content: string;
  readonly placement?: "top" | "right" | "bottom" | "left";
}) {
  const tooltipId = useId();
  const currentDescription = children.props["aria-describedby"];
  const description =
    currentDescription === undefined
      ? tooltipId
      : `${currentDescription} ${tooltipId}`;
  return (
    <span
      className="application-tooltip-host"
      data-tooltip-placement={placement}
    >
      {cloneElement(children, { "aria-describedby": description })}
      <span
        className="application-tooltip-bubble"
        id={tooltipId}
        role="tooltip"
      >
        {content}
      </span>
    </span>
  );
}

function ApplicationToastRegion() {
  const { dismissNotification, notificationState } =
    useApplicationPresentation();
  const visible = notificationState.notifications.slice(-3);
  return (
    <section
      aria-label="Recent application notifications"
      aria-live="polite"
      className="application-toast-region"
    >
      {visible.map((notification) => (
        <ApplicationToast
          dismiss={() => dismissNotification(notification.id)}
          key={`${notification.id}:${notification.sequence}`}
          notification={notification}
        />
      ))}
    </section>
  );
}

function ApplicationToast({
  dismiss,
  notification,
}: {
  readonly dismiss: () => void;
  readonly notification: ApplicationNotification;
}) {
  const [visible, setVisible] = useState(true);
  useEffect(() => {
    const timer = window.setTimeout(() => setVisible(false), 6_000);
    return () => window.clearTimeout(timer);
  }, []);
  if (!visible) return null;
  return (
    <article
      className="application-toast"
      data-tone={notification.tone}
    >
      <div>
        <strong>{notification.title}</strong>
        <p>{notification.message}</p>
      </div>
      {notification.actionLabel === undefined ||
      notification.onAction === undefined ? null : (
        <button onClick={notification.onAction} type="button">
          {notification.actionLabel}
        </button>
      )}
      <button
        aria-label={`Dismiss ${notification.title}`}
        className="application-toast__dismiss"
        onClick={dismiss}
        type="button"
      >
        ×
      </button>
    </article>
  );
}

function ProgressBar({
  progress,
}: {
  readonly progress: ApplicationProgressPresentation;
}) {
  return (
    <div className="application-progress-row">
      <div className="application-progress-row__heading">
        <strong>{progress.label}</strong>
        <span>{progress.status}</span>
      </div>
      {progress.total === null ? (
        <progress aria-label={`Progress for ${progress.label}`} role="progressbar" />
      ) : (
        <progress
          aria-label={`Progress for ${progress.label}`}
          max={progress.total}
          role="progressbar"
          value={progress.completed}
        />
      )}
      <small>
        {progress.detail}
        {progress.percent === null ? "" : `, ${Math.round(progress.percent)}%`}
      </small>
    </div>
  );
}

export function ApplicationFeedbackHub({
  failures,
  onFailureAction,
  progress,
  status,
}: {
  readonly failures: readonly ApplicationFailurePresentation[];
  readonly onFailureAction: (failure: ApplicationFailurePresentation) => void;
  readonly progress: readonly ApplicationProgressPresentation[];
  readonly status: ApplicationOperationalStatus;
}) {
  const {
    clearNotifications,
    dismissNotification,
    notificationState,
  } = useApplicationPresentation();
  const [isOpen, setIsOpen] = useState(false);
  const activityButtonRef = useRef<HTMLButtonElement>(null);
  const activeProgress = progress.filter((item) => item.active);
  const count = failures.length + notificationState.notifications.length;
  const closeCenter = useCallback(() => {
    setIsOpen(false);
    window.requestAnimationFrame(() => activityButtonRef.current?.focus());
  }, []);

  return (
    <aside className="application-feedback-hub">
      {isOpen ? (
        <section
          aria-label="Application notification center"
          className="application-notification-center"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              closeCenter();
            }
          }}
        >
          <header>
            <div>
              <p className="eyebrow">Operational visibility</p>
              <h2>Activity and recovery</h2>
            </div>
            <div className="application-notification-center__actions">
              <button onClick={clearNotifications} type="button">
                Clear notices
              </button>
              <button onClick={closeCenter} type="button">
                Close
              </button>
            </div>
          </header>

          {failures.length === 0 ? null : (
            <section className="application-failure-list">
              <h3>Conditions requiring context</h3>
              {failures.map((failure) => (
                <article
                  className="application-failure-card"
                  data-failure-condition={failure.condition}
                  key={failure.id}
                >
                  <header>
                    <div>
                      <span>{failure.condition.replace("_", " ")}</span>
                      <h4>{failure.title}</h4>
                    </div>
                    <code>{failure.code}</code>
                  </header>
                  <p>{failure.action}</p>
                  <dl>
                    <div>
                      <dt>Category</dt>
                      <dd>{failure.category}</dd>
                    </div>
                    <div>
                      <dt>Source</dt>
                      <dd>{failure.source}</dd>
                    </div>
                    <div>
                      <dt>Revision</dt>
                      <dd>{failure.revision}</dd>
                    </div>
                    {failure.contexts.map((context, index) => (
                      <div key={`${context.label}:${context.value}:${index}`}>
                        <dt>{context.label}</dt>
                        <dd>{context.value}</dd>
                      </div>
                    ))}
                    {failure.lastValidResource === null ? null : (
                      <div>
                        <dt>Last-valid resource</dt>
                        <dd>
                          {failure.lastValidResource.resource} {failure.lastValidResource.identity}, revision {failure.lastValidResource.revision}
                        </dd>
                      </div>
                    )}
                  </dl>
                  <footer>
                    <small>{failure.primaryAction.summary}</small>
                    <button onClick={() => onFailureAction(failure)} type="button">
                      {failure.primaryAction.label}
                    </button>
                  </footer>
                </article>
              ))}
            </section>
          )}

          {progress.length === 0 ? null : (
            <section className="application-progress-list">
              <h3>Progress</h3>
              {progress.map((item) => (
                <ProgressBar key={item.id} progress={item} />
              ))}
            </section>
          )}

          {notificationState.notifications.length === 0 ? (
            <p className="application-notification-center__empty">
              No retained application notices.
            </p>
          ) : (
            <section className="application-notification-list">
              <h3>Notifications</h3>
              {[...notificationState.notifications]
                .reverse()
                .map((notification) => (
                  <article data-tone={notification.tone} key={notification.id}>
                    <div>
                      <strong>{notification.title}</strong>
                      <p>{notification.message}</p>
                    </div>
                    <button
                      aria-label={`Dismiss ${notification.title}`}
                      onClick={() => dismissNotification(notification.id)}
                      type="button"
                    >
                      Dismiss
                    </button>
                  </article>
                ))}
            </section>
          )}
        </section>
      ) : null}

      <section
        aria-label="Application status"
        className="application-feedback-status"
        data-status-condition={status.condition}
      >
        <div className="application-feedback-status__summary" role="status">
          <span className="application-feedback-status__indicator" />
          <div>
            <strong>{status.label}</strong>
            <small>{status.detail}</small>
          </div>
        </div>
        {activeProgress[0] === undefined ? null : (
          <ProgressBar progress={activeProgress[0]} />
        )}
        <button
          aria-expanded={isOpen}
          className="application-feedback-status__notices"
          onClick={() => setIsOpen((current) => !current)}
          ref={activityButtonRef}
          type="button"
        >
          Activity
          <span>{count}</span>
        </button>
      </section>
    </aside>
  );
}
