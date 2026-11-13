using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.Themes.Fluent;
using System;
using System.Text.Json;

namespace Hertzline.D1.Avalonia;

internal static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        if (Environment.GetEnvironmentVariable("HERTZLINE_D1_SMOKE") is not null)
        {
            var ping = IpcStub.Ping("d1-smoke");
            if (!ping.Ok)
            {
                Console.Error.WriteLine("d1 avalonia smoke failed");
                Environment.Exit(1);
                return;
            }

            Console.WriteLine($"d1_avalonia_smoke_ok version={ping.Version}");
            Environment.Exit(0);
            return;
        }

        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .LogToTrace();
}

public sealed class App : Application
{
    public override void Initialize()
    {
        Styles.Add(new FluentTheme());
        RequestedThemeVariant = ThemeVariant.Light;
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            desktop.MainWindow = new MainWindow();
        }

        base.OnFrameworkInitializationCompleted();
    }
}

public sealed class MainWindow : Window
{
    private readonly TextBlock _out;

    public MainWindow()
    {
        Title = "Hertzline D1 Avalonia spike";
        Width = 640;
        Height = 420;

        _out = new TextBlock
        {
            Text = "idle",
            TextWrapping = TextWrapping.Wrap,
        };

        var pingButton = new Button { Content = "Ping stub" };
        pingButton.Click += (_, _) =>
        {
            var res = IpcStub.Ping("d1-avalonia-ui");
            _out.Text = JsonSerializer.Serialize(res);
        };

        Content = new StackPanel
        {
            Margin = new Thickness(24),
            Spacing = 12,
            Orientation = Orientation.Vertical,
            Children =
            {
                new TextBlock { Text = "Hertzline D1", FontSize = 24 },
                new TextBlock { Text = ".NET + Avalonia IPC stub" },
                pingButton,
                _out,
            },
        };
    }
}
