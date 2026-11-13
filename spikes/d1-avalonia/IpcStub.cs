namespace Hertzline.D1.Avalonia;

public sealed record PingResponse(bool Ok, string Message, string Version);

public static class IpcStub
{
    public static PingResponse Ping(string client)
    {
        _ = client;
        return new PingResponse(true, "pong", "0.0.0-d1-avalonia");
    }
}
