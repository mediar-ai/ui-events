import { useState, useEffect, useRef } from "react";
import "./App.css"; // Assuming basic CSS/Tailwind setup

// Import shadcn/ui components (assuming they are set up)
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";

// Define the TypeScript interface matching ui-events/src/event.rs
interface ApplicationInfo {
  name?: string | null;
  pid?: number | null;
}

interface WindowInfo {
  title?: string | null;
  id?: string | null;
}

interface Position {
  x: number;
  y: number;
}

interface Size {
  width: number;
  height: number;
}

interface ElementDetails {
  role?: string | null;
  identifier?: string | null;
  value?: any | null; // Use 'any' for flexibility, matching serde_json::Value
  position?: Position | null;
  size?: Size | null;
}

// Corresponds to the UiEvent struct in Rust
interface UiEvent {
  event_type: string; // Assuming EventType enum translates to string
  timestamp: string; // ISO 8601 string from chrono DateTime<Utc>
  application?: ApplicationInfo | null;
  window?: WindowInfo | null;
  element?: ElementDetails | null;
  event_specific_data?: any | null;
}

function App() {
  const [events, setEvents] = useState<UiEvent[]>([]);
  const [isConnected, setIsConnected] = useState(false);
  const ws = useRef<WebSocket | null>(null);
  const MAX_EVENTS = 100; // Limit the number of events displayed

  useEffect(() => {
    // Connect to the WebSocket server started by the Rust backend
    ws.current = new WebSocket("ws://localhost:9001");

    ws.current.onopen = () => {
      console.log("websocket connected");
      setIsConnected(true);
    };

    ws.current.onclose = () => {
      console.log("websocket disconnected");
      setIsConnected(false);
      // Optional: implement reconnection logic here if needed
    };

    ws.current.onerror = (error) => {
      console.error("websocket error:", error);
      setIsConnected(false);
    };

    ws.current.onmessage = (event) => {
      try {
        // Parse the incoming JSON string into a UiEvent object
        const newEvent: UiEvent = JSON.parse(event.data);
        // Add the new event to the start of the array, keeping only MAX_EVENTS
        setEvents((prevEvents) =>
          [newEvent, ...prevEvents].slice(0, MAX_EVENTS)
        );
      } catch (error) {
        console.error(
          "failed to parse incoming event:",
          error,
          "data:",
          event.data
        );
      }
    };

    // Cleanup function: close the WebSocket connection when the component unmounts
    return () => {
      ws.current?.close();
    };
  }, []); // Empty dependency array ensures this effect runs only once on mount

  return (
    <div className="container mx-auto p-4 h-screen flex flex-col bg-background text-foreground font-mono">
      <Card className="flex-grow flex flex-col overflow-hidden">
        <CardHeader>
          <CardTitle className="text-lg">ui event stream</CardTitle>
          <CardDescription className="flex items-center gap-2 text-xs">
            connection status:
            <Badge
              variant={isConnected ? "default" : "destructive"}
              className="px-1.5 py-0.5 text-xs"
            >
              {isConnected ? "connected" : "disconnected"}
            </Badge>
          </CardDescription>
        </CardHeader>
        <CardContent className="flex-grow overflow-hidden p-0">
          <ScrollArea className="h-full p-4">
            {events.length === 0 ? (
              <p className="text-muted-foreground italic text-center text-sm pt-4">
                waiting for events...
              </p>
            ) : (
              <ul className="space-y-2 text-xs">
                {events.map((event, index) => (
                  <li
                    key={`${event.timestamp}-${index}`}
                    className="border-b pb-2 last:border-b-0 flex flex-col space-y-0.5"
                  >
                    <div className="flex justify-between items-center">
                      <span className="font-semibold text-primary truncate mr-2">
                        {event.event_type}
                      </span>
                      <span className="text-muted-foreground text-right whitespace-nowrap">
                        {new Date(event.timestamp).toLocaleTimeString("en-US", {
                          hour12: false,
                          hour: "2-digit",
                          minute: "2-digit",
                          second: "2-digit",
                        })}
                      </span>
                    </div>
                    <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-muted-foreground">
                      {event.application?.name && (
                        <span className="truncate">
                          app: {event.application.name}
                        </span>
                      )}
                      {event.window?.title && (
                        <span className="truncate">
                          win: {event.window.title}
                        </span>
                      )}
                      {event.element?.identifier && (
                        <span className="truncate">
                          el: {event.element.identifier}
                        </span>
                      )}
                      {event.element?.value &&
                        typeof event.element.value === "string" &&
                        event.element.value.trim() &&
                        event.element.value.length < 50 && (
                          <span className="truncate">
                            val: "{event.element.value}"
                          </span>
                        )}
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}

export default App;
