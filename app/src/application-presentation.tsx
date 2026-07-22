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
import { APPLICATION_SEMANTIC_SURFACES } from "./accessibility-semantics.ts";
import {
  containTabFocus,
  focusFirstInScope,
} from "./focus-management.ts";
import { restoreShellFocus } from "./shell-input.ts";
import { SCREEN_READER_SURFACES } from "./screen-reader-support.ts";
import "./background-jobs.css";

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
    window.requestAnimationFrame(() => restoreShellFocus(returnFocus));
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
    if (menuRef.current !== null) focusFirstInScope(menuRef.current);
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
    if (event.key === "Tab") {
      event.preventDefault();
      containTabFocus(
        event.currentTarget,
        document.activeElement,
        event.shiftKey,
      );
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
        tabIndex={-1}
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
      aria-atomic="false"
      aria-label={APPLICATION_SEMANTIC_SURFACES.notifications.label}
      aria-live="polite"
      aria-relevant="additions text"
      className="application-toast-region"
      id={APPLICATION_SEMANTIC_SURFACES.notifications.id}
      role={APPLICATION_SEMANTIC_SURFACES.notifications.role}
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
  onDismiss,
  onRetry,
  progress,
}: {
  readonly onDismiss?: (id: string) => void;
  readonly onRetry?: (id: string) => void;
  readonly progress: ApplicationProgressPresentation;
}) {
  return (
    <div className="application-progress-row">
      <div className="application-progress-row__heading">
        <strong>{progress.label}</strong>
        <span>{progress.status}</span>
      </div>
      {progress.total === null ? (
        <progress
          aria-describedby={SCREEN_READER_SURFACES.jobs.descriptionId}
          aria-label={`Progress for ${progress.label}`}
          role="progressbar"
        />
      ) : (
        <progress
          aria-describedby={SCREEN_READER_SURFACES.jobs.descriptionId}
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
      {progress.canRetry !== true && progress.canDismiss !== true ? null : (
        <div className="application-progress-row__actions">
          {progress.canRetry === true && onRetry !== undefined ? (
            <button onClick={() => onRetry(progress.id)} type="button">Retry</button>
          ) : null}
          {progress.canDismiss === true && onDismiss !== undefined ? (
            <button onClick={() => onDismiss(progress.id)} type="button">Dismiss</button>
          ) : null}
        </div>
      )}
    </div>
  );
}

export function ApplicationFeedbackHub({
  failures,
  onFailureAction,
  onProgressDismiss,
  onProgressRetry,
  progress,
  status,
}: {
  readonly failures: readonly ApplicationFailurePresentation[];
  readonly onFailureAction: (failure: ApplicationFailurePresentation) => void;
  readonly onProgressDismiss: (id: string) => void;
  readonly onProgressRetry: (id: string) => void;
  readonly progress: readonly ApplicationProgressPresentation[];
  readonly status: ApplicationOperationalStatus;
}) {
  const {
    clearNotifications,
    dismissNotification,
    notificationState,
  } = useApplicationPresentation();
  const [isOpen, setIsOpen] = useState(false);
  const [progressCategory, setProgressCategory] = useState("all");
  const [progressStatus, setProgressStatus] = useState("all");
  const activityButtonRef = useRef<HTMLButtonElement>(null);
  const centerRef = useRef<HTMLElement>(null);
  const centerCloseRef = useRef<HTMLButtonElement>(null);
  const activeProgress = progress.filter((item) => item.active);
  const progressCategories = [...new Set(
    progress.map((item) => item.category ?? "application"),
  )].sort();
  const progressStatuses = [...new Set(progress.map((item) => item.status))].sort();
  const visibleProgress = progress.filter(
    (item) =>
      (progressCategory === "all" ||
        (item.category ?? "application") === progressCategory) &&
      (progressStatus === "all" || item.status === progressStatus),
  );
  const count =
    failures.length + notificationState.notifications.length + progress.length;
  const closeCenter = useCallback(() => {
    setIsOpen(false);
    window.requestAnimationFrame(() =>
      restoreShellFocus(activityButtonRef.current),
    );
  }, []);

  useEffect(() => {
    if (!isOpen) return undefined;
    const frame = window.requestAnimationFrame(() => {
      if (centerRef.current !== null) {
        focusFirstInScope(centerRef.current, centerCloseRef.current);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isOpen]);

  return (
    <aside className="application-feedback-hub">
      {isOpen ? (
        <section
          aria-label="Application notification center"
          aria-describedby={SCREEN_READER_SURFACES.dialogs.descriptionId}
          aria-labelledby="application-notification-center-title"
          className="application-notification-center"
          id="application-notification-center"
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              closeCenter();
            }
          }}
          ref={centerRef}
          role="dialog"
          tabIndex={-1}
        >
          <header>
            <div>
              <p className="eyebrow">Operational visibility</p>
              <h2 id="application-notification-center-title">
                Activity and recovery
              </h2>
            </div>
            <div className="application-notification-center__actions">
              <button onClick={clearNotifications} type="button">
                Clear notices
              </button>
              <button
                onClick={closeCenter}
                ref={centerCloseRef}
                type="button"
              >
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
            <section
              className="application-progress-list"
              aria-describedby={SCREEN_READER_SURFACES.jobs.descriptionId}
              aria-label={SCREEN_READER_SURFACES.jobs.label}
            >
              <header className="application-progress-list__header">
                <h3>Background jobs</h3>
                <div className="application-progress-list__filters">
                  <label>
                    Category
                    <select
                      aria-label="Filter jobs by category"
                      onChange={(event) => setProgressCategory(event.target.value)}
                      value={progressCategory}
                    >
                      <option value="all">All categories</option>
                      {progressCategories.map((category) => (
                        <option key={category} value={category}>{category}</option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Status
                    <select
                      aria-label="Filter jobs by status"
                      onChange={(event) => setProgressStatus(event.target.value)}
                      value={progressStatus}
                    >
                      <option value="all">All statuses</option>
                      {progressStatuses.map((jobStatus) => (
                        <option key={jobStatus} value={jobStatus}>{jobStatus}</option>
                      ))}
                    </select>
                  </label>
                </div>
              </header>
              {visibleProgress.length === 0 ? (
                <p className="application-notification-center__empty">
                  No jobs match the selected filters.
                </p>
              ) : (
                visibleProgress.map((item) => (
                  <ProgressBar
                    key={item.id}
                    onDismiss={onProgressDismiss}
                    onRetry={onProgressRetry}
                    progress={item}
                  />
                ))
              )}
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
        <div
          className="application-feedback-status__summary"
          aria-atomic="true"
          aria-label={APPLICATION_SEMANTIC_SURFACES.applicationStatus.label}
          aria-live="polite"
          id={APPLICATION_SEMANTIC_SURFACES.applicationStatus.id}
          role={APPLICATION_SEMANTIC_SURFACES.applicationStatus.role}
        >
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
          aria-controls="application-notification-center"
          aria-expanded={isOpen}
          aria-haspopup="dialog"
          className="application-feedback-status__notices"
          onClick={() => (isOpen ? closeCenter() : setIsOpen(true))}
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
