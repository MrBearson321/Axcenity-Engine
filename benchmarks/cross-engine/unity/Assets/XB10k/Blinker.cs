// XB10k: wakes every `wait` seconds, reads a grid block and recolors it.
using System.Collections;
using UnityEngine;

public class Blinker : MonoBehaviour
{
    public Transform block;
    public Material material;
    public float wait = 0.1f;
    Color baseColor;
    int count;

    IEnumerator Start()
    {
        baseColor = material.color;
        var delay = new WaitForSeconds(wait);
        while (true)
        {
            yield return delay;
            count++;
            float height = block.position.y;
            material.color = (count % 2 == 0 && height > 0f) ? baseColor : new Color(1f, 0.5f, 0f);
        }
    }
}
