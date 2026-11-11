// XB10k: turns one spinner every frame.
using UnityEngine;

public class Spinner : MonoBehaviour
{
    public float speed = 30f;
    float angle;

    void Update()
    {
        angle = (angle + speed * Time.deltaTime) % 360f;
        transform.rotation = Quaternion.Euler(0f, angle, 0f);
    }
}
